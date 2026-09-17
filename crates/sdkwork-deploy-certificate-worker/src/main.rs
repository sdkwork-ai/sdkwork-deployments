use sdkwork_deploy_certificate_worker::{
    CertificateIssuanceWorker, CertificateIssuanceWorkerConfig,
};
use sdkwork_deploy_service_host::bootstrap_deploy_service_host_from_env;
use tokio::signal;

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let config = CertificateIssuanceWorkerConfig::from_env()
        .expect("certificate issuance worker configuration failed");
    // The full service host, not the bare repository: issuance reads the certificate
    // and seals its material through the same ports an operator's request uses, so a
    // reduced host would quietly skip the trust-anchor and master-key checks.
    let host = bootstrap_deploy_service_host_from_env()
        .await
        .expect("certificate issuance worker bootstrap failed");
    CertificateIssuanceWorker::new(host.service, config)
        .run_until_shutdown(shutdown_signal())
        .await;
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
