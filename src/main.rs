#[forbid(clippy::unwrap_in_result)]
mod config;
mod server;
mod utils;

// TODO remove clone as much as possible and restructure code accordingly
// TODO create benchmarks, tests
// TODO create UI for this
// TODO make live reloading optional
// TODO allow individual config files & folders

use clap::Parser;
use std::path::PathBuf;

use tokio::signal;
use tokio::time::{Duration, interval};
use tracing::{Level, info};

use crate::config::ConfigFile;
use crate::server::MockServer;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, help = "Path to configuration files directory")]
    configs_path: Option<PathBuf>,
    #[clap(
        default_value_t = false,
        long,
        short = 'v',
        help = "Enable verbose logging"
    )]
    verbose: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Server error occurred: {0}")]
    ServerError(#[from] server::MockServerError),
    #[error("Configuration error occurred: {0}")]
    ConfigError(#[from] config::ConfigError),
    #[error("IO error occurred: {0}")]
    IoError(#[from] std::io::Error),
}

pub type AppResult<T> = Result<T, AppError>;

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Args::parse();

    let level = if args.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(level).init();

    let config = ConfigFile::load(args.configs_path)?;

    let mut server = MockServer::new(config);
    server.start_all().await?;

    let mut tick = interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Shutdown signal received.");
                break;
            }
            _ = tick.tick() => {
                server.health().await?;
            }
        }
    }

    server.stop_all().await?;
    Ok(())
}
