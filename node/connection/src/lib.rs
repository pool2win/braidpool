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

use futures::{SinkExt, StreamExt};
use std::marker::Unpin;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::{error::Error, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub mod connection_manager;

use connection_manager::ConnectionManager;
use connection_manager::Metadata;
use protocol::{HandshakeMessage, ProtocolMessage};
use tokio::sync::broadcast;

type Sender = mpsc::Sender<Bytes>;
type Receiver = mpsc::Receiver<Bytes>;
type SendToAllSender = broadcast::Sender<Bytes>;
type SendToAllReceiver = broadcast::Receiver<Bytes>;

/// Split provided stream, create an internal channel and spawn tasks
/// to use the reader, writer, and channel sender, receiver.
///
/// Decoupling the read from tcp stream to handling messages prevents
/// long running tasks stalling further reads. It also allows spawning
/// tasks to handle individual messages - if needed.
async fn start_connection(
    stream: TcpStream,
    addr: SocketAddr,
    manager: Arc<ConnectionManager>,
    max_pending_messages: usize,
    send_to_all_receiver: SendToAllReceiver,
    notifier: Arc<Notify>,
) -> JoinHandle<()> {
    let (r, w) = stream.into_split();
    let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
    let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
    let (sender, mut receiver) = mpsc::channel::<Bytes>(max_pending_messages);
    tokio::spawn(async move {
        start(
            addr,
            &mut framed_reader,
            &mut framed_writer,
            sender,
            &mut receiver,
            manager,
            send_to_all_receiver,
            notifier,
        )
        .await;
    })
}

/// Connect to a peer.
/// Each new connect spawns two new task from `start`.
pub fn connect(
    peer: String,
    manager: Arc<ConnectionManager>,
    max_pending_messages: usize,
    send_to_all_receiver: SendToAllReceiver,
    notifier: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        log::info!("Connecting to peer: {:?}", peer);
        let stream = TcpStream::connect(peer.as_str())
            .await
            .expect("Error connecting to peer");
        if let Ok(addr_iter) = peer.to_socket_addrs() {
            if let Some(addr) = addr_iter.into_iter().next() {
                start_connection(
                    stream,
                    addr,
                    manager,
                    max_pending_messages,
                    send_to_all_receiver,
                    notifier,
                )
                .await;
            }
        }
    })
}

/// Start listening on provided interface and port as the addr parameter.
///
/// addr is of the form "host:port".
/// Each new accept returns spawns two new task using `start_connection`.
pub async fn start_listen(
    addr: String,
    manager: Arc<ConnectionManager>,
    max_pending_messages: usize,
    send_to_all: SendToAllSender,
    notifier: Arc<Notify>,
) {
    log::info!("Binding to {}", addr);
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            loop {
                // Asynchronously wait for an inbound TcpStream.
                log::info!("Starting accept");
                notifier.notify_one();
                match listener.accept().await {
                    Ok((stream, peer_address)) => {
                        log::info!("Accepted connection from {:?}", peer_address);
                        start_connection(
                            stream,
                            peer_address,
                            manager.clone(),
                            max_pending_messages,
                            send_to_all.subscribe(),
                            notifier.clone(),
                        )
                        .await;
                    }
                    Err(e) => log::error!("Couldn't get client on accept: {:?}", e),
                }
            }
        }
        Err(e) => {
            log::info!("Failed to listen {:?}", e);
        }
    }
}

/// Start a read loop
/// Reading from tcp stream and writer to internal bounded channel sender.
///
/// Returns errors if downstream is unable to keep pace with messages
/// received. This helps handle DDoS attacks by peers flooding the connection.
///
/// Returns an error also if the peer has closed connection and we
/// can't read any more from the socket.
async fn start_read_loop<R>(reader: &mut R, channel_sender: Sender) -> Result<(), Box<dyn Error>>
where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
{
    loop {
        log::debug!("Read loop....");
        if let Some(message) = reader.next().await {
            match message {
                Ok(message) => {
                    log::debug!("Read message.... {:?}", message.len());
                    match channel_sender.try_send(message.freeze()) {
                        Ok(_) => {
                            log::debug!("Message sent on channel...");
                        }
                        Err(error) => match error {
                            TrySendError::Full(_) => {
                                log::info!("Sender flooding channel");
                                return Err("Sender flooding channel".into());
                            }
                            TrySendError::Closed(_) => {
                                log::info!("Receiver closed for channel");
                                return Err("Receiver closed for channel".into());
                            }
                        },
                    }
                }
                Err(_) => {
                    return Err("Message receive: peer closed connection".into());
                }
            }
        } else {
            return Err("Receive: peer closed connection".into());
        }
    }
}

/// Start message handler
///
/// Reads received messages from internal bounded channel and writes
/// response obtained from Protocol
async fn start_message_handler<W>(
    writer: &mut W,
    channel_receiver: &mut Receiver,
    mut send_to_all_receiver: SendToAllReceiver,
    notifier: Arc<Notify>,
) -> Result<(), Box<dyn Error>>
where
    W: SinkExt<Bytes> + Unpin + Sync + Send,
{
    loop {
        tokio::select! {
            Some(msg) = channel_receiver.recv() => {
                if handle_received(writer, msg).await.is_err() {
                    return Err("Message handling failure: Closing peer connection".into());
                } else {
                    notifier.notify_one();
                }
            },
            Ok(msg_bytes) = send_to_all_receiver.recv() => {
                if writer.send(msg_bytes).await.is_err() {
                    return Err("Send failed: Closing peer connection".into());
                }
            }
        };
    }
}

async fn handle_received<W>(writer: &mut W, msg: Bytes) -> Result<(), Box<dyn Error>>
where
    W: SinkExt<Bytes> + Unpin + Sync + Send,
{
    let message: protocol::Message;
    if let Ok(msg) = protocol::Message::from_bytes(&msg) {
        message = msg;
    } else {
        return Err("Error parsing message".into());
    }
    // TODO: These nesting of unwraps is not good. We should get rid of the nesting.
    match message.response_for_received() {
        Ok(result) => {
            if let Some(response) = result {
                if let Some(to_send) = response.as_bytes() {
                    if (writer.send(to_send).await).is_err() {
                        return Err("Send failed: Closing peer connection".into());
                    }
                } else {
                    return Err("Error serializing: Closing peer connection".into());
                }
            }
        }
        Err(_) => {
            return Err("Error constructing response: Closing peer connection".into());
        }
    }
    Ok(())
}

/// Start read loop and message handler and wait for either of them to
/// return. When one returns, closes the writer and the
/// receiver. These calls to close will result in the spawned tasks
/// aborting.
async fn start<R, W>(
    addr: SocketAddr,
    reader: &mut R,
    writer: &mut W,
    channel_sender: Sender,
    channel_receiver: &mut Receiver,
    manager: Arc<ConnectionManager>,
    send_to_all_receiver: SendToAllReceiver,
    notifier: Arc<Notify>,
) where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
    W: SinkExt<Bytes> + Unpin + Send + Sync,
{
    log::info!("Spawning tasks");
    if manager
        .insert(
            addr,
            Metadata {
                created_at: SystemTime::now(),
            },
        )
        .is_err()
    {
        log::info!("Connection refused - limit reached");
        return;
    }
    let message = HandshakeMessage::start(&addr).unwrap();
    let _ = writer.send(message.as_bytes().unwrap()).await;
    tokio::select! {
        _ = start_read_loop(reader, channel_sender) => {
            log::debug!("Read loop returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
        _ = start_message_handler(writer, channel_receiver, send_to_all_receiver, notifier) => {
            log::debug!("Message handler returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
    };
    // Cleanup - first close tcpstream, then remove from connection manager
    let _ = writer.close().await;
    manager.remove(&addr);
}

/// Start a task to send heartbeats every given duration period.
///
/// Heartbeats are set back when certain message types are sent.
pub async fn start_heartbeat(
    addr: String,
    duration: u64,
    sender: broadcast::Sender<Bytes>,
    manager: Arc<ConnectionManager>,
) -> (Arc<Notify>, JoinHandle<()>) {
    log::debug!("Socket address {:?}", addr);
    let socket_addr: SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();
    let message = protocol::HeartbeatMessage::start(&socket_addr)
        .unwrap()
        .as_bytes()
        .unwrap();
    let mut interval = time::interval(Duration::from_millis(duration));
    let notify = Arc::new(Notify::new());
    let notify_from_others = notify.clone();

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if manager.num_connections() > 0 {
                        sender.send(message.clone()).expect("Error sending heartbeat. Quitting.");
                    }
                }
                _ = notify.notified() => {
                    interval.reset();
                }
            }
        }
    });
    (notify_from_others, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr};
    use tokio_util::bytes::Bytes;

    #[tokio::test]
    async fn it_should_run_connect_without_errors() {
        let _ = env_logger::try_init();

        // listen and client are from different clients, and therefore we need two different connection managers.
        let listen_manager = Arc::new(ConnectionManager::new(3));
        let connect_manager = Arc::new(ConnectionManager::new(3));

        let listen_manager_cloned = listen_manager.clone();
        let connect_manager_cloned = connect_manager.clone();

        let (send_to_all_tx, send_to_all_rx) = broadcast::channel::<Bytes>(32);

        let notify = Arc::new(Notify::new());
        let notify_listen_cloned = notify.clone();
        let notify_connect_cloned = notify.clone();

        tokio::spawn(async move {
            start_listen(
                "localhost:6680".to_string(),
                listen_manager_cloned,
                32,
                send_to_all_tx,
                notify_listen_cloned,
            )
            .await;
        });

        notify.notified().await;

        let _ = connect(
            "localhost:6680".to_string(),
            connect_manager_cloned,
            32,
            send_to_all_rx,
            notify_connect_cloned,
        )
        .await;

        notify.notified().await;

        assert_eq!(listen_manager.num_connections(), 1);
        assert_eq!(connect_manager.num_connections(), 1);
    }

    #[tokio::test]
    async fn it_should_read_message_from_stream_and_send_to_channel_sender() {
        let _ = env_logger::try_init();
        let _writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];
        let mut reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        tokio::spawn(async move {
            let _ = start_read_loop(&mut reader_iter, sender).await;
        });

        let received = receiver.recv().await.unwrap();
        assert_eq!(Some(received), message.as_bytes());
    }

    #[tokio::test]
    async fn it_should_read_message_from_stream_and_handle_error_when_sending_to_channel_fails() {
        let _ = env_logger::try_init();
        let _writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];
        let mut reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        receiver.close();

        let received = start_read_loop(&mut reader_iter, sender).await;
        assert!(received.is_err());
    }

    #[tokio::test]
    async fn it_should_stop_loop_if_message_is_an_error() {
        let _ = env_logger::try_init();
        let _writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let _msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> =
            vec![Err(std::io::Error::new(ErrorKind::Other, "oh no!"))];
        let mut reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        receiver.close();

        let received = start_read_loop(&mut reader_iter, sender).await;
        log::debug!("{:?}", received);
        assert!(received.is_err());
    }

    #[tokio::test]
    async fn it_should_stop_loop_when_stream_returns_none() {
        let _ = env_logger::try_init();
        let _writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let _msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
        let mut reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        receiver.close();

        let received = start_read_loop(&mut reader_iter, sender).await;
        log::debug!("{:?}", received);
        assert!(received.is_err());
    }

    #[tokio::test]
    async fn it_should_handle_message_received_on_channel() {
        let _ = env_logger::try_init();
        let mut writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let _msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
        let mut _reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        let (_, send_to_all_rx) = broadcast::channel::<Bytes>(32);

        let spawn_handle = tokio::spawn(async move {
            let r = start_message_handler(
                &mut writer,
                &mut receiver,
                send_to_all_rx,
                Arc::new(Notify::new()),
            )
            .await;
            assert_eq!(
                r.unwrap_err().to_string(),
                "Error parsing message".to_string()
            );
        });

        // first send a message that is handled correctly
        let _ = sender.send(message.as_bytes().unwrap()).await;

        // then send a message that can't be parsed and results in channel closing
        let _ = sender.send(Bytes::from("hello world")).await;
        let _ = spawn_handle.await;
    }

    #[tokio::test]
    async fn it_should_add_to_connection_manager_on_starting_connection() {
        let _ = env_logger::try_init();
        let mut writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let _msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
        let mut _reader_iter = stream::iter(reader);

        let (sender, mut receiver) = mpsc::channel::<Bytes>(3);
        let sender_cloned = sender.clone();

        let start_manager = Arc::new(ConnectionManager::new(3));
        let start_manager_cloned = start_manager.clone();

        let (_, send_to_all_rx) = broadcast::channel::<Bytes>(32);

        let spawn_handle = tokio::spawn(async move {
            start(
                addr,
                &mut _reader_iter,
                &mut writer,
                sender_cloned,
                &mut receiver,
                start_manager_cloned,
                send_to_all_rx,
                Arc::new(Notify::new()),
            )
            .await;
        });

        // first send a message that is handled correctly
        let _ = sender.send(message.as_bytes().unwrap()).await;

        // then send a message that can't be parsed and results in channel closing
        let _ = sender.send(Bytes::from("hello world")).await;
        let _ = spawn_handle.await;
        assert_eq!(start_manager.num_connections(), 0);
    }

    #[tokio::test]
    async fn it_should_shutdown_connection_if_peer_is_flooding_it() {
        let _ = env_logger::try_init();
        let mut writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let msg_bytes_2 = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes), Ok(msg_bytes_2)];
        let mut reader_iter = stream::iter(reader);

        // limit the channel capacity to one message
        let (sender, mut receiver) = mpsc::channel::<Bytes>(1);
        let sender_cloned = sender.clone();

        let start_manager = Arc::new(ConnectionManager::new(3));
        let start_manager_cloned = start_manager.clone();

        let (_, send_to_all_rx) = broadcast::channel::<Bytes>(32);

        let spawn_handle = tokio::spawn(async move {
            start(
                addr,
                &mut reader_iter,
                &mut writer,
                sender_cloned,
                &mut receiver,
                start_manager_cloned,
                send_to_all_rx,
                Arc::new(Notify::new()),
            )
            .await;
        });

        let _ = spawn_handle.await;

        assert!(sender.is_closed());
        assert_eq!(start_manager.num_connections(), 0);
    }

    #[tokio::test]
    async fn it_should_start_heartbeat_without_errors_and_handle_ticks() {
        use tokio::time::sleep;
        let _ = env_logger::try_init();
        let _writer: Vec<Bytes> = vec![];

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(&addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let msg_bytes_2 = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes), Ok(msg_bytes_2)];
        let _reader_iter = stream::iter(reader);

        // limit the channel capacity to one message
        let (sender, mut receiver) = broadcast::channel::<Bytes>(10);
        let _sender_cloned = sender.clone();

        let start_manager = Arc::new(ConnectionManager::new(3));
        let local = "127.0.0.1:8080".parse().unwrap();
        let s = Metadata {
            created_at: SystemTime::now(),
        };
        assert!(start_manager.insert(local, s).unwrap().is_none());

        let (_, _send_to_all_rx) = broadcast::channel::<Bytes>(32);

        let (notifier, handle) = start_heartbeat(addr.to_string(), 1, sender, start_manager).await;

        // notify once
        notifier.notify_one();

        let test_handle = tokio::spawn(async move {
            let msg = receiver.recv().await;
            assert!(msg.is_ok());
        });

        // wait for 5 ms, i.e. 5 interval ticks
        sleep(Duration::from_millis(5)).await;

        handle.abort();

        let _ = tokio::join!(handle, test_handle);
    }
}
