use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use std::marker::Unpin;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::SystemTime;
use std::{error::Error, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::connection_manager::ConnectionManager;
use crate::connection_manager::Metadata;
use crate::protocol::{self, HandshakeMessage, ProtocolMessage};

const CHANNEL_CAPACITY: usize = 32;

type Sender = mpsc::Sender<Bytes>;
type Receiver = mpsc::Receiver<Bytes>;

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
) -> JoinHandle<()> {
    let (r, w) = stream.into_split();
    let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
    let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
    let (sender, mut receiver) = mpsc::channel::<Bytes>(CHANNEL_CAPACITY);
    tokio::spawn(async move {
        start(
            addr,
            &mut framed_reader,
            &mut framed_writer,
            sender,
            &mut receiver,
            manager,
        )
        .await;
    })
}

/// Connect to a peer.
/// Each new connect spawns two new task from `start`.
pub fn connect(peer: String, manager: Arc<ConnectionManager>) -> JoinHandle<()> {
    tokio::spawn(async move {
        log::info!("Connecting to peer: {:?}", peer);
        let stream = TcpStream::connect(peer.as_str())
            .await
            .expect("Error connecting to peer");
        if let Ok(addr_iter) = peer.to_socket_addrs() {
            if let Some(addr) = addr_iter.into_iter().next() {
                start_connection(stream, addr, manager).await;
            }
        }
    })
}

/// Start listening on provided interface and port as the addr parameter.
///
/// addr is of the form "host:port".
/// Each new accept returns spawns two new task using `start_connection`.
pub async fn start_listen(addr: String, manager: Arc<ConnectionManager>) {
    log::info!("Binding to {}", addr);
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            loop {
                // Asynchronously wait for an inbound TcpStream.
                log::info!("Starting accept");
                match listener.accept().await {
                    Ok((stream, peer_address)) => {
                        log::info!("Accepted connection from {:?}", peer_address);
                        start_connection(stream, peer_address, manager.clone()).await;
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
                        Err(TrySendError::Full(_)) => {
                            log::info!("Sender flooding channel");
                            return Err("Sender flooding channel".into());
                        }
                        Err(TrySendError::Closed(_)) => {
                            log::info!("Receiver closed for channel");
                            return Err("Receiver closed for channel".into());
                        }
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
) -> Result<(), Box<dyn Error>>
where
    W: SinkExt<Bytes> + Unpin + Sync + Send,
{
    while let Some(msg) = channel_receiver.recv().await {
        log::info!("Received message");
        let message: protocol::Message;
        if let Ok(msg) = protocol::Message::from_bytes(&msg) {
            message = msg;
        } else {
            return Err("Error parsing message".into());
        }
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
    }
    log::debug!("returning from start message handler...");
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
) where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
    W: SinkExt<Bytes> + Unpin + Send + Sync,
{
    log::info!("Spawning tasks");
    manager.insert(
        addr,
        Metadata {
            created_at: SystemTime::now(),
        },
    );
    let message = HandshakeMessage::start(&addr).unwrap();
    let _ = writer.send(message.as_bytes().unwrap()).await;
    tokio::select! {
        _ = start_read_loop(reader, channel_sender) => {
            log::debug!("Read loop returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
        _ = start_message_handler(writer, channel_receiver) => {
            log::debug!("Message handler returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
    };
    // Cleanup - first close tcpstream, then remove from connection manager
    let _ = writer.close().await;
    manager.remove(&addr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    // TODO: This is a smoke test and we need to improve how we wait for listen to complete before we invoke connect.
    async fn it_should_run_connect_without_errors() {
        use tokio::time::{sleep, Duration};

        let _ = env_logger::try_init();

        // listen and client are from different clients, and therefore we need two different connection managers.
        let listen_manager = Arc::new(ConnectionManager::new());
        let connect_manager = Arc::new(ConnectionManager::new());

        let listen_manager_cloned = listen_manager.clone();
        let connect_manager_cloned = connect_manager.clone();

        tokio::spawn(async move {
            start_listen("localhost:25188".to_string(), listen_manager_cloned).await;
        });

        // TODO: Fix this smoke test! Kill the sleep in this smoke test.
        sleep(Duration::from_millis(100)).await;

        let _ = connect("localhost:25188".to_string(), connect_manager_cloned).await;

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

        let spawn_handle = tokio::spawn(async move {
            let r = start_message_handler(&mut writer, &mut receiver).await;
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

        let start_manager = Arc::new(ConnectionManager::new());
        let start_manager_cloned = start_manager.clone();

        let spawn_handle = tokio::spawn(async move {
            start(
                addr,
                &mut _reader_iter,
                &mut writer,
                sender_cloned,
                &mut receiver,
                start_manager_cloned,
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

        let start_manager = Arc::new(ConnectionManager::new());
        let start_manager_cloned = start_manager.clone();

        let spawn_handle = tokio::spawn(async move {
            start(
                addr,
                &mut reader_iter,
                &mut writer,
                sender_cloned,
                &mut receiver,
                start_manager_cloned,
            )
            .await;
        });

        let _ = spawn_handle.await;

        assert!(sender.is_closed());
        assert_eq!(start_manager.num_connections(), 0);
    }
}
