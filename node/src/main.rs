use clap::Parser;
use log;
use std::error::Error;

mod cli;
mod connection;
mod protocol;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let datadir = args.datadir;
    log::info!("Using braid data directory: {}", datadir.display());

    setup_logging()?;
    setup_tracing()?;

    if let Some(addpeer) = args.addpeer {
        for peer in addpeer {
            tokio::spawn(async move {
                connection::connect(peer.as_str()).await;
            });
        }
    }

    let _ = connection::start_listen(args.bind).await;
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
