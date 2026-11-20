use sdkwork_deploy_factory_reconciliation_worker::{
    interval_from_env, FactoryReconciliationWorker,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let database_url = std::env::var("SDKWORK_DEPLOY_DATABASE_URL")
        .or_else(|_| std::env::var("SDKWORK_DATABASE_URL"))
        .expect("SDKWORK_DEPLOY_DATABASE_URL or SDKWORK_DATABASE_URL is required");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect deploy database for factory reconciliation failed");
    // Read-only worker: the lookup constructor carries no secret key and no
    // control-plane lease.
    let repository = Arc::new(DeployRepository::new_lookup(pool));
    let worker = FactoryReconciliationWorker::new(repository, interval_from_env());
    info!(
        interval_seconds = worker.period().as_secs(),
        "factory reconciliation worker configured"
    );
    worker.run_until_shutdown(shutdown_signal()).await;
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler for factory reconciliation worker");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler for factory reconciliation worker")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
}
