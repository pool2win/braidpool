use bytes::{Bytes, BytesMut};
use std::{error::Error, net::SocketAddr};
//use tokio::sync::mpsc;
use futures::{SinkExt, StreamExt};
use std::marker::Unpin;
use std::net::ToSocketAddrs;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

// const CHANNEL_CAPACITY: usize = 32;

use crate::protocol::{self, HandshakeMessage, Message, ProtocolMessage};

/// Connect to a peer. Creates a new Connection and calls its
/// `start_from_connect` method.
/// Each new connect spawns a new task.
pub fn connect(peer: String) {
    tokio::spawn(async move {
        log::info!("Connecting to peer: {:?}", peer);
        let stream = TcpStream::connect(peer.as_str())
            .await
            .expect("Error connecting");
        let (r, w) = stream.into_split();
        let framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
        let framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
        let mut conn = Connection::new(framed_reader, framed_writer);
        if let Ok(addr_iter) = peer.to_socket_addrs() {
            if let Some(addr) = addr_iter.into_iter().next() {
                if conn.start_from_connect(&addr).await.is_err() {
                    log::info!("Peer closed connection");
                }
            }
        }
    });
}

/// Start listening on provided interface and port as the addr parameter
/// addr is of the form "host:port".
///
/// Each accept return is handled by a Connection and its
/// `start_from_accept` method.
pub async fn start_listen(addr: String) {
    log::info!("Binding to {}", addr);
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            loop {
                // Asynchronously wait for an inbound TcpStream.
                log::info!("Starting accept");
                match listener.accept().await {
                    Ok((stream, _)) => {
                        log::debug!("Accepted connection");
                        let (r, w) = stream.into_split();
                        let framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
                        let framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
                        let mut conn = Connection::new(framed_reader, framed_writer);

                        tokio::spawn(async move {
                            if conn.start_from_accept().await.is_err() {
                                log::info!("Peer closed connection")
                            }
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

/// Connection captures the reader and writer for a connection.
///
/// When a connection is closed the instance is dropped by going out
/// of scope in the creating `connect/start_listen` functions.
pub struct Connection<R, W> {
    reader: R,
    writer: W,
    //channel_receiver: mpsc::Receiver<String>,
    //channel_sender: mpsc::Sender<String>,
}

impl<R, W> Connection<R, W>
where
    R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
    W: SinkExt<Bytes> + Unpin,
{
    /// Create a new connection with the given reader and writer.
    pub fn new(r: R, w: W) -> Self {
        //let (channel_sender, channel_receiver) = mpsc::channel(CHANNEL_CAPACITY);
        Connection {
            reader: r,
            writer: w,
            // channel_receiver,
            // channel_sender,
        }
    }

    /// Once the Connection is setup send the initial handshake protocol messages.
    ///
    /// Replies to all protocols are handled in the `start_read_loop`
    /// and the `message_received` methods.
    pub async fn start_from_connect(&mut self, addr: &SocketAddr) -> Result<(), Box<dyn Error>> {
        log::info!("Starting from connect");
        let message = HandshakeMessage::start(addr).unwrap();
        let _ = self.writer.send(message.as_bytes().unwrap()).await;
        self.start_read_loop().await
    }

    /// When a peer connects it will send the handshake method. So
    /// this method just starts the read loop for now.
    ///
    /// TODO(pool2win): Drop connection if there is no message from a
    /// peer in timeout period.
    pub async fn start_from_accept(&mut self) -> Result<(), Box<dyn Error>> {
        log::info!("Starting from accept");
        self.start_read_loop().await
    }

    /// Start a read loop. For now this method directly calls
    /// `message_received` which sends any required responses.
    ///
    /// TODO(pool2win): Push received messages to a channel. Start
    /// tasks to consume these received messages. We will need to send
    /// a reference to the Connection over the channel, or we'll need
    /// a way to enqueue responses for Connection to send back.
    pub async fn start_read_loop(&mut self) -> Result<(), Box<dyn Error>> {
        log::debug!("Start read loop....");
        loop {
            let item = self.reader.next().await;
            if let Some(message) = item {
                match message {
                    Ok(message) => {
                        if self.message_received(&message.freeze()).await.is_err() {
                            return Err("send: peer closed connection".into());
                        }
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

    /// Handles received messages by parsing the message and
    /// demultiplexing to appropriate protocol.
    /// See TODO for `start_read_loop`.
    async fn message_received(&mut self, msg: &Bytes) -> Result<(), &'static str> {
        let message: Message = protocol::Message::from_bytes(msg).unwrap();
        log::debug!("{:?}", message);
        match message.response_for_received() {
            Ok(result) => {
                if let Some(response) = result {
                    if let Some(to_send) = response.as_bytes() {
                        if (self.writer.send(to_send).await).is_err() {
                            return Err("Send failed: Closing peer connection");
                        }
                    } else {
                        return Err("Error serializing: Closing peer connection");
                    }
                }
            }
            Err(_) => {
                return Err("Error constructing response: Closing peer connection");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

    #[tokio::test]
    async fn it_should_create_connection_using_framed_read_and_write_without_errors() {
        let (r, w) = tokio::io::duplex(64);
        let mut framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
        let mut framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());

        let msg = Bytes::from("Hello World!");
        let _ = framed_writer.send(msg.clone()).await;
        let result = framed_reader.next().await;

        assert!(result.unwrap().is_ok_and(|rr| rr == msg[..]));

        Connection::new(framed_reader, framed_writer);
    }

    #[tokio::test]
    async fn it_should_create_connection_using_streams_on_vectors_without_errors() {
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        Connection::new(reader_iter, writer);
    }

    #[tokio::test]
    async fn it_should_write_bytes_to_writer_succesfully() {
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![];
        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        let mut connection = Connection::new(reader_iter, writer);

        let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(addr).unwrap();
        let msg_bytes = message.as_bytes().unwrap();

        assert!(connection.writer.send(msg_bytes).await.is_ok());
        assert_eq!(connection.writer.len(), 1);
        assert_eq!(connection.writer[0], message.as_bytes().unwrap()); // length delimited codec not used by test reader/writer
    }

    #[tokio::test]
    async fn it_should_read_bytes_from_reader_succesfully() {
        let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());

        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];
        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        let mut connection = Connection::new(reader_iter, writer);

        let read_result = connection.reader.next().await;
        assert!(read_result.is_some());
        let result = read_result.unwrap().expect("Error reading result");
        assert_eq!(result, message.as_bytes().unwrap());
    }

    #[tokio::test]
    async fn it_should_start_read_loop() {
        let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        let mut connection = Connection::new(reader_iter, writer);

        assert!(connection.start_read_loop().await.is_err()); // reading from limited vec results in None in the end
    }

    #[tokio::test]
    async fn it_should_start_from_connect() {
        let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        let mut connection = Connection::new(reader_iter, writer);

        assert!(connection.start_from_connect(addr).await.is_err()); // reading from limited vec results in None in the end
    }

    #[tokio::test]
    async fn it_should_start_from_accept() {
        let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let message = HandshakeMessage::start(addr).unwrap();
        let msg_bytes = BytesMut::from_iter(message.as_bytes().unwrap().iter());
        let reader: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(msg_bytes)];

        let writer: Vec<Bytes> = vec![];

        let reader_iter = stream::iter(reader);
        let mut connection = Connection::new(reader_iter, writer);

        assert!(connection.start_from_accept().await.is_err()); // reading from limited vec results in None in the end
    }

    #[tokio::test]
    #[ignore]
    // TODO(pool2win) - Enable test for connect and start_listen once we have channels setup
    async fn it_should_start_listen_without_errors() {
        let _ = env_logger::try_init();
        let listen_handle =
            tokio::spawn(async move { start_listen("localhost:25188".to_string()).await });

        connect("localhost:25188".to_string());
        let _ = listen_handle.await;
    }
}
