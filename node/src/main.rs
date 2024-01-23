use bytes::Bytes;
use clap::Parser;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::broadcast;

mod cli;
mod config;
mod connection;
mod connection_manager;
mod protocol;

use crate::connection_manager::ConnectionManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let config = config::load_config_from_file(args.config_file).unwrap();
    let network_config = config.network.unwrap();
    let peer_config = config.peer.unwrap();

    setup_logging()?;
    setup_tracing()?;

    let manager = Arc::new(ConnectionManager::new(peer_config.max_peer_count.unwrap()));

    let (send_to_all_tx, _) =
        broadcast::channel::<Bytes>(peer_config.max_pending_send_to_all.unwrap());

    if let Some(seeds) = peer_config.seeds {
        for seed in seeds {
            let send_to_all_rx = send_to_all_tx.subscribe();
            connection::connect(
                seed,
                manager.clone(),
                peer_config.max_pending_messages.unwrap(),
                send_to_all_rx,
            );
        }
    }

    let mut bind_address = network_config.bind.unwrap();
    bind_address.push(':');
    bind_address.push_str(network_config.port.unwrap().to_string().as_str());
    connection::start_listen(
        bind_address,
        manager.clone(),
        peer_config.max_pending_messages.unwrap(),
        send_to_all_tx,
    )
    .await;
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
