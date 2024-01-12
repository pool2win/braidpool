use bytes::Bytes;
use clap::Parser;
use std::error::Error;
use tokio::sync::mpsc;

mod cli;
mod connection;
mod connection_manager;
mod protocol;

const CHANNEL_CAPACITY: usize = 32;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let datadir = args.datadir;
    log::info!("Using braid data directory: {}", datadir.display());

    setup_logging()?;
    setup_tracing()?;

    let (sender, mut _receiver) = mpsc::channel::<Bytes>(CHANNEL_CAPACITY);

    if let Some(addpeer) = args.addpeer {
        for peer in addpeer {
            connection::connect(peer, sender.clone());
        }
    }

    connection::start_listen(args.bind, &sender.clone()).await;
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
