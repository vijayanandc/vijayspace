//! The `weft` binary: `serve` (API + worker), `api`, `worker`, `migrate` (and the internal
//! `script-child` that runs one custom function under OS limits).

use clap::{Parser, Subcommand};
use tokio_util::sync::CancellationToken;
use weft_server::{config::Settings, heartbeat, wiring};
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
    /// Internal: run one custom function from stdin under OS limits (started by the worker).
    #[command(name = "script-child", hide = true)]
    ScriptChild,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::ScriptChild)) {
        // Before any runtime or thread exists, so the memory cap covers everything.
        return weft_server::script_child::run();
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli))
}

/// Ctrl+C, or SIGTERM (what `docker stop`, Kubernetes and systemd send): the API stops accepting requests and
/// finishes those in flight, and the worker stops claiming and lets running jobs finish (bounded).
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

/// `RUST_LOG`, else `WEFT_LOG` (default `info,sqlx=warn`); `WEFT_LOG_FORMAT=json` for one JSON object per line
/// (log shipping), else readable text.
fn init_logging(source: &weft_server::properties::Source) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        source
            .get("WEFT_LOG")
            .unwrap_or_else(|| "info,sqlx=warn".into())
            .into()
    });
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if source.get("WEFT_LOG_FORMAT").as_deref() == Some("json") {
        builder.json().init();
    } else {
        builder.init();
    }
}

/// Scaling out multiplies connections: warn when this server's pool would take MySQL past 90% of its limit.
async fn warn_if_pools_exceed_mysql(pool: &weft_db::Pool, pool_size: u32) {
    match weft_db::connection_headroom(pool).await {
        Ok((max, open)) if u64::from(open) + u64::from(pool_size) > u64::from(max) * 9 / 10 => {
            tracing::warn!(
                max_connections = max,
                open,
                pool_size,
                "MySQL is close to its connection limit: raise max_connections or lower WEFT_DB_POOL_SIZE \
                 (every server opens up to its pool size)"
            );
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "reading MySQL's connection limit"),
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let source = weft_server::properties::Source::load().map_err(anyhow::Error::msg)?;
    init_logging(&source);
    let command = cli.command.unwrap_or(Command::Serve);
    let serves_api = matches!(command, Command::Serve | Command::Api);
    let settings = Settings::load(&source, serves_api).map_err(anyhow::Error::msg)?;
    let pool = weft_db::connect(&settings.database_url, settings.db_pool_size).await?;
    weft_db::migrate(&pool).await?;
    if matches!(command, Command::Migrate) {
        tracing::info!("migrations applied");
        return Ok(());
    }
    warn_if_pools_exceed_mysql(&pool, settings.db_pool_size).await;
    let deps = wiring::build(&settings, weft_db::MySqlStore::new(pool))?;
    let shutdown = CancellationToken::new();
    let kind = match command {
        Command::Api => "api",
        Command::Worker => "worker",
        Command::Serve | Command::Migrate | Command::ScriptChild => "all",
    };
    let me = heartbeat::me(&deps, kind);
    let worker_config = WorkerConfig {
        id: format!("worker-{}", me.id.simple()),
        concurrency: settings.worker_concurrency,
    };
    let beats = heartbeat::spawn(deps.clone(), me, shutdown.clone());
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
            weft_api::ApiState::new(deps.clone()),
            &settings.client_origins,
        );
        let listener = tokio::net::TcpListener::bind(&settings.bind).await?;
        tracing::info!(bind = %settings.bind, public_url = %settings.public_url, "API listening");
        let token = shutdown.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_signal().await;
                token.cancel();
            })
            .await?;
    } else {
        shutdown_signal().await;
        shutdown.cancel();
    }
    if let Some(w) = worker {
        let _ = w.await;
    }
    let _ = beats.await;
    Ok(())
}
