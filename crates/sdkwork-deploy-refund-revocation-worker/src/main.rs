use sdkwork_deploy_refund_revocation_worker::revoke_once;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info};

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn interval() -> Duration {
    std::env::var("SDKWORK_DEPLOY_REFUND_REVOCATION_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(3600))
}

#[tokio::main]
async fn main() {
    init_tracing();
    let database_url = std::env::var("SDKWORK_DEPLOY_DATABASE_URL")
        .or_else(|_| std::env::var("SDKWORK_DATABASE_URL"))
        .expect("SDKWORK_DEPLOY_DATABASE_URL or SDKWORK_DATABASE_URL is required");
    let apply = !matches!(std::env::args().nth(1).as_deref(), Some("--dry-run"));
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect deploy database for refund revocation failed");
    let period = interval();
    info!(interval_seconds = period.as_secs(), apply, "refund revocation worker started");

    loop {
        match revoke_once(&pool, apply).await {
            Ok(report) => {
                info!(
                    candidates = report.candidates.len(),
                    archived = report.archived,
                    "refund revocation pass"
                );
            }
            Err(error) => error!(%error, "refund revocation pass failed"),
        }
        tokio::select! {
            () = tokio::time::sleep(period) => {},
            () = shutdown_signal() => break,
        }
    }
    info!("refund revocation worker stopped");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
