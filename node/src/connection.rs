use bytes::Bytes;
use std::{error::Error, net::SocketAddr};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
//use tokio::sync::mpsc;
use std::net::ToSocketAddrs;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

// const CHANNEL_CAPACITY: usize = 32;

use crate::protocol::{self, HandshakeMessage, Message, ProtocolMessage};

/// Connect to a peer and start read loop
pub async fn connect(peer: &str) {
    log::info!("Connecting to peer: {:?}", peer);
    let stream = TcpStream::connect(peer).await.expect("Error connecting");
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
}

pub async fn start_listen(addr: String) -> Result<(), Box<dyn Error>> {
    log::info!("Binding to {}", addr);
    let listener = TcpListener::bind(addr).await?;
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

pub struct Connection {
    reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    //channel_receiver: mpsc::Receiver<String>,
    //channel_sender: mpsc::Sender<String>,
}

impl Connection {
    pub fn new(
        reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
        writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    ) -> Connection {
        //let (channel_sender, channel_receiver) = mpsc::channel(CHANNEL_CAPACITY);
        Connection {
            reader,
            writer,
            // channel_receiver,
            // channel_sender,
        }
    }

    pub async fn start_from_connect(&mut self, addr: &SocketAddr) -> Result<(), Box<dyn Error>> {
        use futures::SinkExt;
        log::info!("Starting from connect");
        let message = HandshakeMessage::start(addr).unwrap();
        self.writer.send(message.as_bytes().unwrap()).await?;
        self.start_read_loop().await?;
        Ok(())
    }

    pub async fn start_from_accept(&mut self) -> Result<(), Box<dyn Error>> {
        log::info!("Starting from accept");
        self.start_read_loop().await?;
        Ok(())
    }

    pub async fn start_read_loop(&mut self) -> Result<(), Box<dyn Error>> {
        use futures::StreamExt;
        log::debug!("Start read loop....");
        loop {
            match self.reader.next().await {
                None => {
                    return Err("peer closed connection".into());
                }
                Some(item) => match item {
                    Err(_) => {
                        return Err("peer closed connection".into());
                    }
                    Ok(message) => {
                        if self.message_received(&message.freeze()).await.is_err() {
                            return Err("peer closed connection".into());
                        }
                    }
                },
            }
        }
    }

    async fn message_received(&mut self, message: &Bytes) -> Result<(), &'static str> {
        use futures::SinkExt;

        let message: Message = protocol::Message::from_bytes(message).unwrap();
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
