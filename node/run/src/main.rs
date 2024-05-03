// Copyright 2024 Braidpool Developers

// This file is part of Braidpool

// Braidpool is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Braidpool is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Braidpool. If not, see <https://www.gnu.org/licenses/>.

use clap::Parser;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::bytes::Bytes;

mod cli;

use connection::connection_manager::ConnectionManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let config = config::load_config_from_file(args.config_file).unwrap();
    let bind_address = config::get_bind_address(config.network);

    setup_logging()?;
    setup_tracing()?;

    let manager = Arc::new(ConnectionManager::new(config.peer.max_peer_count));

    let (send_to_all_tx, _) = broadcast::channel::<Bytes>(config.peer.max_pending_send_to_all);
    let connect_broadcast_sender = send_to_all_tx.clone();
    let listen_broadcast_sender = send_to_all_tx.clone();

    let (reset_notifier, _) = connection::start_heartbeat(
        bind_address.clone(),
        config.peer.heartbeat_interval,
        send_to_all_tx,
        manager.clone(),
    )
    .await;

    for seed in config.peer.seeds {
        connection::connect(
            seed,
            manager.clone(),
            config.peer.max_pending_messages,
            connect_broadcast_sender.subscribe(),
            reset_notifier.clone(),
        );
    }

    connection::start_listen(
        bind_address,
        manager.clone(),
        config.peer.max_pending_messages,
        listen_broadcast_sender,
        reset_notifier,
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
