use clap::Parser;
use std::error::Error;
use std::net::ToSocketAddrs;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

mod cli;
mod connection;
mod protocol;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let datadir = args.datadir;
    println!("Using braid data directory: {}", datadir.display());

    setup_tracing()?;

    if let Some(addnode) = args.addnode {
        for node in addnode.iter() {
            //println!("Connecting to node: {:?}", node);
            let stream = TcpStream::connect(node).await.expect("Error connecting");
            let (r, w) = stream.into_split();
            let framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
            let framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
            let mut conn = connection::Connection::new(framed_reader, framed_writer);
            if let Ok(addr_iter) = node.to_socket_addrs() {
                if let Some(addr) = addr_iter.into_iter().next() {
                    tokio::spawn(async move {
                        if conn.start_from_connect(&addr).await.is_err() {
                            println!("Peer closed connection")
                        }
                    });
                }
            }
        }
    }

    println!("Binding to {}", args.bind);
    let listener = TcpListener::bind(&args.bind).await?;
    loop {
        // Asynchronously wait for an inbound TcpStream.
        println!("Starting accept");
        match listener.accept().await {
            Ok((stream, _)) => {
                println!("\n\naccepted connection");
                let (r, w) = stream.into_split();
                let framed_reader = FramedRead::new(r, LengthDelimitedCodec::new());
                let framed_writer = FramedWrite::new(w, LengthDelimitedCodec::new());
                let mut conn = connection::Connection::new(framed_reader, framed_writer);

                tokio::spawn(async move {
                    if conn.start_from_accept().await.is_err() {
                        println!("Peer closed connection")
                    }
                });
            }
            Err(e) => println!("couldn't get client: {:?}", e),
        }
    }
}

fn setup_tracing() -> Result<(), Box<dyn Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}
