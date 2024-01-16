use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use std::marker::Unpin;
use std::net::ToSocketAddrs;
use std::{error::Error, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::protocol::{self, HandshakeMessage, ProtocolMessage};

const CHANNEL_CAPACITY: usize = 32;

type Sender = mpsc::Sender<Bytes>;
type Receiver = mpsc::Receiver<Bytes>;

/// Connect to a peer.
/// Each new connect spawns two new task from `start`.
pub fn connect(peer: String) -> JoinHandle<()> {
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
    })
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
        if let Some(message) = reader.next().await {
            match message {
                Ok(message) => {
                    log::debug!("read message.... {:?}", message.len());
                    if channel_sender.send(message.freeze()).await.is_err() {
                        return Err("Error handling received message".into());
                    }
                    log::debug!("Message sent on channel...");
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

/// Once a connection is setup send the initial handshake protocol
/// messages and start the read loop as well as task to handle
/// received messages.
///
/// Decoupling the read from tcp stream to handling messages prevents
/// long running tasks stalling further reads. It also allows spawning
/// tasks to handle individual messages - if needed.
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
            log::debug!("Read loop returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
        _ = start_message_handler(writer, channel_receiver) => {
            log::debug!("Message handler returned: Closing connection to {:?}", addr);
            channel_receiver.close();
        }
    };
    let _ = writer.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr};
    // use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

    // #[tokio::test]
    // async fn it_should_run_connect_without_errors() {
    //     let _ = env_logger::try_init();

    //     tokio::spawn(async {
    //         start_listen("localhost:25188".to_string()).await;
    //     });
    //     log::debug!("abc");

    //     let connect_handle = connect("localhost:25188".to_string());
    //     assert!(connect_handle.await.is_ok());
    // }

    // #[tokio::test]
    // async fn it_should_start_read_loop_with_vector_streams() {
    //     let (r, w) = tokio::io::duplex(64);
    //     let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
    //     let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());

    //     let msg = Bytes::from("Hello World!");
    //     let _ = framed_writer.send(msg.clone()).await;
    //     let result = framed_reader.next().await;

    //     assert!(result.unwrap().is_ok_and(|rr| rr == msg[..]));

    //     let (channel_sender, channel_receiver) = mpsc::channel::<Bytes>(3);

    //     // 1. listen to channel - when message received, terminate test

    //     // 2. spawn read loop
    //     tokio::spawn(async move {
    //         let _ = start_read_loop(&mut framed_reader, sender).await;
    //     });

    //     // 3. finally send a message on the channel

    //     // 4. write more tests to handle error branches
    // }

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
}

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
