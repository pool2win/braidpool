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
    pub fn new(r: R, w: W) -> Self {
        //let (channel_sender, channel_receiver) = mpsc::channel(CHANNEL_CAPACITY);
        Connection {
            reader: r,
            writer: w,
            // channel_receiver,
            // channel_sender,
        }
    }

    pub async fn start_from_connect(&mut self, addr: &SocketAddr) -> Result<(), Box<dyn Error>> {
        log::info!("Starting from connect");
        let message = HandshakeMessage::start(addr).unwrap();
        let _ = self.writer.send(message.as_bytes().unwrap()).await;
        self.start_read_loop().await?;
        Ok(())
    }

    pub async fn start_from_accept(&mut self) -> Result<(), Box<dyn Error>> {
        log::info!("Starting from accept");
        self.start_read_loop().await?;
        Ok(())
    }

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
                        return Err("receive: peer closed connection".into());
                    }
                }
            } else {
                return Err("receive: peer closed connection".into());
            }
        }
    }

    async fn message_received(&mut self, message: &Bytes) -> Result<(), &'static str> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

    #[tokio::test]
    async fn it_create_reader_and_writer_from_vector() {
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
    async fn it_should_create_connection_using_streams_on_vectors() {}
}
