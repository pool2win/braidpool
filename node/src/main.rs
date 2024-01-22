use clap::Parser;
use std::error::Error;
use std::sync::Arc;

mod cli;
mod config;
mod connection;
mod connection_manager;
mod protocol;

use crate::connection_manager::ConnectionManager;
const CONNECTION_LIMIT: usize = 32;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let datadir = args.datadir;
    log::info!("Using braid data directory: {}", datadir.display());

    setup_logging()?;
    setup_tracing()?;

    let manager = Arc::new(ConnectionManager::new(CONNECTION_LIMIT));

    if let Some(addpeer) = args.addpeer {
        for peer in addpeer {
            connection::connect(peer, manager.clone());
        }
    }

    connection::start_listen(args.bind, manager.clone()).await;
    log::debug!("Listen done");
    Ok(())
}

fn setup_tracing() -> Result<(), Box<dyn Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

fn setup_logging() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    Ok(())
}
