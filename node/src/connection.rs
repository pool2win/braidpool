use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use std::marker::Unpin;
use std::net::ToSocketAddrs;
use std::{error::Error, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::protocol::{self, HandshakeMessage, Message, ProtocolMessage};

const CHANNEL_CAPACITY: usize = 32;

type Sender = mpsc::Sender<Bytes>;
type Receiver = mpsc::Receiver<Bytes>;

/// Connect to a peer.
/// Each new connect spawns two new task from `start`.
pub fn connect(peer: String) {
    tokio::spawn(async move {
        log::info!("Connecting to peer: {:?}", peer);
        let (sender, mut receiver) = mpsc::channel::<Bytes>(CHANNEL_CAPACITY);
        let stream = TcpStream::connect(peer.as_str())
            .await
            .expect("Error connecting");
        let (r, w) = stream.into_split();
        let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
        let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
        if let Ok(addr_iter) = peer.to_socket_addrs() {
            if let Some(addr) = addr_iter.into_iter().next() {
                tokio::spawn(async move {
                    start(
                        addr,
                        &mut framed_reader,
                        &mut framed_writer,
                        sender,
                        &mut receiver,
                    )
                    .await;
                });
            }
        }
    });
}

/// Start listening on provided interface and port as the addr parameter
/// addr is of the form "host:port".
/// Each new accept returns spawns two new task from `start`.
pub async fn start_listen(addr: String) {
    log::info!("Binding to {}", addr);
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            loop {
                // Asynchronously wait for an inbound TcpStream.
                log::info!("Starting accept");
                match listener.accept().await {
                    Ok((stream, peer_address)) => {
                        log::info!("Accepted connection from {:?}", peer_address);
                        let (r, w) = stream.into_split();
                        let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
                        let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
                        let (sender, mut receiver) = mpsc::channel::<Bytes>(CHANNEL_CAPACITY);
                        tokio::spawn(async move {
                            start(
                                peer_address,
                                &mut framed_reader,
                                &mut framed_writer,
                                sender,
                                &mut receiver,
                            )
                            .await;
                        });
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

/// Start a read loop, reading from tcp stream and writer to bounded channel sender.
pub async fn start_read_loop<R>(
    reader: &mut R,
    channel_sender: Sender,
) -> Result<(), Box<dyn Error>>
where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
{
    log::debug!("Start read loop....");
    loop {
        log::debug!("Read loop ....");
        let item = reader.next().await;
        if let Some(message) = item {
            match message {
                Ok(message) => {
                    log::debug!("read message.... {:?}", message.len());
                    let _ = channel_sender.send(message.freeze()).await;
                    log::debug!("Message sent on channel...");
                    // if self.message_received(&message.freeze()).await.is_err() {
                    //     return Err("send: peer closed connection".into());
                    // }
                }
                Err(_) => {
                    return Err("message receive: peer closed connection".into());
                }
            }
        } else {
            return Err("receive: peer closed connection".into());
        }
    }
}

async fn start_message_handler<W>(writer: &mut W, channel_receiver: &mut Receiver)
where
    W: SinkExt<Bytes> + Unpin + Sync + Send,
{
    while let Some(msg) = channel_receiver.recv().await {
        log::info!("Received message");
        let message: Message = protocol::Message::from_bytes(&msg).unwrap();
        log::debug!("{:?}", message);
        match message.response_for_received() {
            Ok(result) => {
                if let Some(response) = result {
                    if let Some(to_send) = response.as_bytes() {
                        if (writer.send(to_send).await).is_err() {
                            log::info!("Send failed: Closing peer connection");
                        }
                    } else {
                        log::info!("Error serializing: Closing peer connection");
                    }
                }
            }
            Err(_) => {
                log::info!("Error constructing response: Closing peer connection");
            }
        }
    }
}

/// Once the Connection is setup send the initial handshake protocol messages.
///
/// Replies to all protocols are handled in the `start_read_loop`
/// and the `message_received` methods.
pub async fn start<R, W>(
    addr: SocketAddr,
    reader: &mut R,
    writer: &mut W,
    channel_sender: Sender,
    channel_receiver: &mut Receiver,
) where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
    W: SinkExt<Bytes> + Unpin + Send + Sync,
{
    log::info!("Starting from connect");
    let message = HandshakeMessage::start(&addr).unwrap();
    let _ = writer.send(message.as_bytes().unwrap()).await;
    tokio::select! {
        _ = start_read_loop(reader, channel_sender) => {
        }
        _ = start_message_handler(writer, channel_receiver) => {
        }
    };
    log::info!("Closing connection to {:?}", addr);
    channel_receiver.close();
    let _ = writer.close().await;
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use bytes::Bytes;
//     use futures::stream;
//     use std::net::{IpAddr, Ipv4Addr, SocketAddr};
//     use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

//     #[tokio::test]
//     async fn it_should_create_connection_using_framed_read_and_write_without_errors() {
//         let (r, w) = tokio::io::duplex(64);
//         let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
//         let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());

//         let msg = Bytes::from("Hello World!");
//         let _ = framed_writer.send(msg.clone()).await;
//         let result = framed_reader.next().await;

//         assert!(result.unwrap().is_ok_and(|rr| rr == msg[..]));

//         let (sender, _) = mpsc::channel::<Bytes>(3);

//         Connection::new(framed_reader, framed_writer, sender);
//     }

//     #[tokio::test]
//     async fn it_should_create_connection_using_streams_on_vectors_without_errors() {
//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);

//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         Connection::new(reader_iter, writer, sender);
//     }

//     #[tokio::test]
//     async fn it_should_write_bytes_to_writer_succesfully() {
//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);
//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         let mut connection = Connection::new(reader_iter, writer, sender);

//         let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//         let message = HandshakeMessage::start(addr).unwrap();
//         let msg_bytes = message.as_bytes().unwrap();

//         assert!(connection.writer.send(msg_bytes).await.is_ok());
//         assert_eq!(connection.writer.len(), 1);
//         assert_eq!(connection.writer[0], message.as_bytes().unwrap()); // length delimited codec not used by test reader/writer
//     }

//     #[tokio::test]
//     async fn it_should_read_bytes_from_reader_succesfully() {
//         let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//         let message = HandshakeMessage::start(addr).unwrap();
//         let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());

//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];
//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);
//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         let mut connection = Connection::new(reader_iter, writer, sender);

//         let read_result = connection.reader.next().await;
//         assert!(read_result.is_some());
//         let result = read_result.unwrap().expect("Error reading result");
//         assert_eq!(result, message.as_bytes().unwrap());
//     }

//     #[tokio::test]
//     async fn it_should_start_read_loop() {
//         let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//         let message = HandshakeMessage::start(addr).unwrap();
//         let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);
//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         let mut connection = Connection::new(reader_iter, writer, sender);

//         assert!(connection.start_read_loop().await.is_err()); // reading from limited vec results in None in the end
//     }

//     #[tokio::test]
//     async fn it_should_start_from_connect() {
//         let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//         let message = HandshakeMessage::start(addr).unwrap();
//         let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);
//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         let mut connection = Connection::new(reader_iter, writer, sender);

//         assert!(connection.start_from_connect(addr).await.is_err()); // reading from limited vec results in None in the end
//     }

//     #[tokio::test]
//     async fn it_should_start_from_accept() {
//         let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//         let message = HandshakeMessage::start(addr).unwrap();
//         let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
//         let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

//         let writer: Vec<Bytes> = vec![];

//         let reader_iter = stream::iter(reader);
//         let (sender, _) = mpsc::channel::<Bytes>(3);
//         let mut connection = Connection::new(reader_iter, writer, sender);

//         assert!(connection.start_from_accept().await.is_err()); // reading from limited vec results in None in the end
//     }

//     #[tokio::test]
//     #[ignore]
//     // TODO(pool2win) - Enable test for connect and start_listen once we have channels setup
//     async fn it_should_start_listen_without_errors() {
//         let _ = env_logger::try_init();
//         let (sender, _) = mpsc::channel::<Bytes>(3);

//         let sender_for_listen = sender.clone();
//         let listen_handle = tokio::spawn(async move {
//             start_listen("localhost:25188".to_string(), &sender_for_listen).await
//         });

//         connect("localhost:25188".to_string(), sender.clone());
//         let _ = listen_handle.await;
//     }
// }
