//! Bounded reachability probe for a remix Git repository.
//!
//! # Why an HTTP probe and not `git ls-remote`
//!
//! The deploy service hosts no git binary and must not depend on one: a
//! control-plane container that shells out to `git` inherits its credential
//! helpers, its config and its exit-code vocabulary, none of which is a contract
//! this service can keep. Reachability is therefore established the only way an
//! unauthenticated caller can establish it — one bounded HTTP request — and the
//! verdict is limited to what that request can prove.
//!
//! # What this probe deliberately is not
//!
//! It does not clone, does not authenticate, and does not parse the response
//! body. A `200` on a repository landing page proves the host answers for that
//! path; it does not prove that `git clone` will succeed, that the recorded ref
//! exists, or that the credential in `credentialSecretRef` is accepted. That
//! boundary is stated in the API description rather than papered over here.
//!
//! # Bounds
//!
//! One probe costs at most [`GIT_REPOSITORY_PROBE_TIMEOUT`]: the client timeout,
//! the connect timeout and an outer `tokio::time::timeout` all use it, and
//! redirects are capped at [`GIT_REPOSITORY_PROBE_REDIRECT_LIMIT`] hops, so a
//! hostile or broken host cannot hold an API request open or bounce it forever.
//!
//! `HEAD` is the first method because a repository URL that answers `HEAD` proves
//! reachability without transferring a body. Servers that refuse `HEAD` (`405` /
//! `501`) are retried once with `GET`, still inside the same budget.

use std::time::Duration;

use async_trait::async_trait;

/// The wall-clock budget of one probe, shared by every bound below.
pub const GIT_REPOSITORY_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Redirect hops followed before the probe gives up.
pub const GIT_REPOSITORY_PROBE_REDIRECT_LIMIT: usize = 5;

/// What one bounded probe observed.
///
/// The status code is preserved rather than collapsed into a boolean because the
/// rule that interprets it (`sdkwork_deploy_core::classify_git_repository_probe`)
/// distinguishes "reachable" from "reachable but private" from "absent", and the
/// stored error code has to name which one it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitRepositoryProbeResult {
    /// The server answered with this status.
    Status(u16),
    /// No answer within the budget.
    TimedOut,
    /// The connection failed before a status existed. `detail` is for the log:
    /// it can contain a host name and a TLS error, and never reaches the wire.
    NetworkFailure(String),
}

/// Outbound reachability probe for a Git repository URL.
///
/// A port rather than a free function so a deployment (or a test) can supply its
/// own transport, and so the service never constructs an HTTP client per request.
#[async_trait]
pub trait GitRepositoryProbePort: Send + Sync {
    /// Probes `repo_url` once and answers what was observed.
    ///
    /// The caller has already decided the URL is an `http(s)` URL
    /// (`sdkwork_deploy_core::plan_git_repository_probe`): this layer never
    /// reasons about URL shapes and never retries beyond the single `HEAD` →
    /// `GET` fallback described on the module.
    async fn probe(&self, repo_url: &str) -> GitRepositoryProbeResult;
}

/// The production probe: one pooled client, bounded redirects, bounded time.
pub struct BoundedGitRepositoryProbe {
    client: reqwest::Client,
    timeout: Duration,
}

impl BoundedGitRepositoryProbe {
    /// Builds the probe with the standard budget.
    pub fn new() -> Result<Self, String> {
        Self::with_timeout(GIT_REPOSITORY_PROBE_TIMEOUT)
    }

    /// Builds the probe with an explicit budget.
    ///
    /// Exposed so a deployment can tighten the budget and so the tests can hold a
    /// timeout without waiting five seconds. A zero budget is refused: it would
    /// make every probe fail as timed out, which reads as "the repository is
    /// down" instead of "this deployment is misconfigured".
    pub fn with_timeout(timeout: Duration) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err("git repository probe timeout must be greater than zero".to_owned());
        }
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(
                GIT_REPOSITORY_PROBE_REDIRECT_LIMIT,
            ))
            .build()
            .map_err(|error| format!("build the git repository probe HTTP client: {error}"))?;
        Ok(Self { client, timeout })
    }

    /// `HEAD`, falling back to `GET` only when the server refuses the method.
    ///
    /// The fallback is not a retry: it exists because a `405`/`501` answer says
    /// nothing about the repository, only about the method, and reporting it as
    /// "unreachable" would be wrong for every host that disables `HEAD`.
    async fn probe_attempt(&self, repo_url: &str) -> GitRepositoryProbeResult {
        match self.send(reqwest::Method::HEAD, repo_url).await {
            GitRepositoryProbeResult::Status(status) if status == 405 || status == 501 => {
                self.send(reqwest::Method::GET, repo_url).await
            }
            observation => observation,
        }
    }

    async fn send(&self, method: reqwest::Method, repo_url: &str) -> GitRepositoryProbeResult {
        match tokio::time::timeout(self.timeout, self.client.request(method, repo_url).send()).await
        {
            Ok(Ok(response)) => GitRepositoryProbeResult::Status(response.status().as_u16()),
            Ok(Err(error)) => {
                if error.is_timeout() {
                    GitRepositoryProbeResult::TimedOut
                } else {
                    tracing::debug!(
                        "git repository probe transport failure for {repo_url}: {error}"
                    );
                    GitRepositoryProbeResult::NetworkFailure(error.to_string())
                }
            }
            Err(_) => GitRepositoryProbeResult::TimedOut,
        }
    }
}

#[async_trait]
impl GitRepositoryProbePort for BoundedGitRepositoryProbe {
    async fn probe(&self, repo_url: &str) -> GitRepositoryProbeResult {
        // The outer bound covers the HEAD *and* the GET fallback: without it the
        // worst case would be two full budgets, which is not the budget the API
        // documents.
        match tokio::time::timeout(self.timeout, self.probe_attempt(repo_url)).await {
            Ok(observation) => observation,
            Err(_) => GitRepositoryProbeResult::TimedOut,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::thread;

    /// Serves `responses` one connection at a time, in order, then stops.
    ///
    /// Loopback rather than a public host: the probe is the thing under test, and
    /// a test that needs the internet fails for reasons that have nothing to do
    /// with this code.
    fn spawn_scripted_server(responses: Vec<String>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("read loopback address");
        thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        address
    }

    /// Accepts one connection and never answers, so the probe has to time out.
    fn spawn_silent_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("read loopback address");
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                thread::sleep(Duration::from_secs(2));
                drop(stream);
            }
        });
        address
    }

    fn response(status_line: &str) -> String {
        format!("HTTP/1.1 {status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    fn probe() -> BoundedGitRepositoryProbe {
        BoundedGitRepositoryProbe::with_timeout(Duration::from_millis(250))
            .expect("build a bounded loopback probe")
    }

    fn url(address: SocketAddr) -> String {
        format!("http://{address}/sdkwork/app.git")
    }

    #[tokio::test]
    async fn a_reachable_repository_is_reported_with_its_status() {
        let address = spawn_scripted_server(vec![response("200 OK")]);
        assert_eq!(
            probe().probe(&url(address)).await,
            GitRepositoryProbeResult::Status(200)
        );
    }

    #[tokio::test]
    async fn a_private_repository_is_reported_as_an_authentication_challenge() {
        let address = spawn_scripted_server(vec![response("401 Unauthorized")]);
        assert_eq!(
            probe().probe(&url(address)).await,
            GitRepositoryProbeResult::Status(401)
        );
    }

    #[tokio::test]
    async fn an_absent_repository_is_reported_with_its_status() {
        let address = spawn_scripted_server(vec![response("404 Not Found")]);
        assert_eq!(
            probe().probe(&url(address)).await,
            GitRepositoryProbeResult::Status(404)
        );
    }

    #[tokio::test]
    async fn a_server_that_refuses_head_is_probed_once_with_get() {
        let address =
            spawn_scripted_server(vec![response("405 Method Not Allowed"), response("200 OK")]);
        assert_eq!(
            probe().probe(&url(address)).await,
            GitRepositoryProbeResult::Status(200),
            "a 405 on HEAD says nothing about the repository, so GET must answer for it"
        );
    }

    #[tokio::test]
    async fn a_host_that_never_answers_times_out_within_the_budget() {
        let address = spawn_silent_server();
        let started = std::time::Instant::now();
        assert_eq!(
            probe().probe(&url(address)).await,
            GitRepositoryProbeResult::TimedOut
        );
        assert!(
            started.elapsed() < GIT_REPOSITORY_PROBE_TIMEOUT,
            "a silent host must be cut off by the probe budget, not by the test harness"
        );
    }

    #[tokio::test]
    async fn a_host_with_nothing_listening_never_reports_a_status() {
        // Bind and release a port so nothing is listening on it.
        let address = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
            listener.local_addr().expect("read loopback address")
        };
        // Which failure this is depends on the host's TCP stack rather than on
        // this code: a closed loopback port answers RST on most systems (a
        // connection error) but can leave the connect pending until the budget
        // expires on others. Both are "not reachable", which is the contract the
        // caller needs; the stored error code differs (`..._NETWORK_FAILURE` vs
        // `..._TIMEOUT`) and both map to `INVALID`.
        match probe().probe(&url(address)).await {
            GitRepositoryProbeResult::NetworkFailure(detail) => {
                assert!(
                    !detail.is_empty(),
                    "the failure detail is what makes the log actionable"
                );
            }
            GitRepositoryProbeResult::TimedOut => {}
            other => panic!("nothing is listening, so no status can be observed: {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_redirect_loop_is_stopped_at_the_configured_hop_limit() {
        // Every hop answers 302 back to itself. One HEAD request is followed by at
        // most `GIT_REPOSITORY_PROBE_REDIRECT_LIMIT` hops before the client gives
        // up; the server is scripted for the maximum, with one spare.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("read loopback address");
        let location = format!("http://{address}/sdkwork/app.git");
        thread::spawn(move || {
            for _ in 0..=GIT_REPOSITORY_PROBE_REDIRECT_LIMIT {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let redirect = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\
                     Connection: close\r\n\r\n"
                );
                let _ = stream.write_all(redirect.as_bytes());
                let _ = stream.flush();
            }
        });
        match probe().probe(&url(address)).await {
            GitRepositoryProbeResult::NetworkFailure(detail) => {
                assert!(
                    detail.to_ascii_lowercase().contains("redirect"),
                    "the failure must name the redirect bound: {detail}"
                );
            }
            other => panic!("a redirect loop must be bounded, got {other:?}"),
        }
    }

    #[test]
    fn a_zero_budget_is_refused_because_it_would_misreport_every_repository() {
        assert!(BoundedGitRepositoryProbe::with_timeout(Duration::ZERO).is_err());
    }
}
