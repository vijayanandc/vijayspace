//! The `weft` binary: `serve` (API + worker), `api`, `worker`, `migrate`.

use clap::{Parser, Subcommand};
use tokio_util::sync::CancellationToken;
use weft_server::{config::Settings, wiring};
use weft_worker::WorkerConfig;

#[derive(Debug, Parser)]
#[command(name = "weft", about = "Weft integration platform")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// API and worker in one process (default; development and small installs).
    Serve,
    /// HTTP API only.
    Api,
    /// Background worker only.
    Worker,
    /// Apply database migrations and exit.
    Migrate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();
    let cli = Cli::parse();
    let settings = Settings::from_env().map_err(anyhow::Error::msg)?;
    let pool = weft_db::connect(&settings.database_url, 20).await?;
    weft_db::migrate(&pool).await?;
    let command = cli.command.unwrap_or(Command::Serve);
    if matches!(command, Command::Migrate) {
        tracing::info!("migrations applied");
        return Ok(());
    }
    let deps = wiring::build(&settings, weft_db::MySqlStore::new(pool))?;
    let shutdown = CancellationToken::new();
    let worker_config = WorkerConfig {
        id: format!("worker-{}", uuid::Uuid::new_v4().simple()),
        concurrency: settings.worker_concurrency,
    };
    let worker = matches!(command, Command::Serve | Command::Worker).then(|| {
        tokio::spawn(weft_worker::run(
            deps.clone(),
            worker_config,
            shutdown.clone(),
        ))
    });
    let api = matches!(command, Command::Serve | Command::Api);
    if api {
        let app = weft_api::router(
            weft_api::ApiState { deps: deps.clone() },
            &settings.client_origins,
        );
        let listener = tokio::net::TcpListener::bind(&settings.bind).await?;
        tracing::info!(bind = %settings.bind, public_url = %settings.public_url, "API listening");
        let token = shutdown.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = tokio::signal::ctrl_c().await;
                token.cancel();
            })
            .await?;
    } else {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.cancel();
    }
    if let Some(w) = worker {
        let _ = w.await;
    }
    Ok(())
}
