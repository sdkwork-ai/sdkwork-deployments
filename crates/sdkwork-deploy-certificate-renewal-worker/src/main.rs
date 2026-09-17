use sdkwork_deploy_certificate_renewal_worker::{
    CertificateRenewalWorker, CertificateRenewalWorkerConfig,
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
    let config = CertificateRenewalWorkerConfig::from_env()
        .expect("certificate renewal worker configuration failed");
    // The full service host, not the bare repository: the sweep opens orders
    // through the same CAA-checked path an operator's request takes, so running
    // against a reduced host would quietly skip that check.
    let host = bootstrap_deploy_service_host_from_env()
        .await
        .expect("certificate renewal worker bootstrap failed");
    CertificateRenewalWorker::new(host.service, config)
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
