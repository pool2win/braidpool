use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use tokio::sync::mpsc;

type Sender = mpsc::Sender<Bytes>;
type Map = HashMap<SocketAddr, Sender>;

pub struct ConnectionManager {
    lock: Mutex<Map>,
    map: Map,
}

/// ConnectionManager struct maps a peer's IP address to the sender
/// channels.
///
/// To send a message to a peer, we grab the sender channel and
/// enqueue the message on the channel. A different task will take
/// these messages from the sender and send them out to TcpStream for
/// the peer.
impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {
            /// Use a Mutex here as changes to this table will be
            /// infrequent. We can replace with crossbeam SkipMap if
            /// this becomes a performance bottleneck.
            lock: Mutex::new(Map::new()),
            map: Map::new(),
        }
    }

    /// Insert a new peer's IP address and sender channel
    pub fn insert(&mut self, addr: SocketAddr, s: Sender) -> Option<Sender> {
        let _ = self.lock.lock();
        self.map.insert(addr, s)
    }

    /// Remove a peer's IP address
    pub fn remove(&mut self, addr: &SocketAddr) -> Option<Sender> {
        let _ = self.lock.lock();
        self.map.remove(addr)
    }

    /// Find the sender channel for a peer's SocketAddr
    pub fn get(&mut self, addr: &SocketAddr) -> Option<&Sender> {
        let _ = self.lock.lock(); // read only, still grab the mutex
        self.map.get(addr)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn it_should_create_connections_map() {
        let conns = ConnectionManager::new();
        assert_eq!(conns.map.len(), 0);
    }

    #[test]
    pub fn it_should_insert_new_address() {
        let mut conns = ConnectionManager::new();
        let (s, _) = mpsc::channel(10);
        let local = "127.0.0.1:8080".parse().unwrap();
        let other = "127.0.0.1:8081".parse().unwrap();

        assert!(conns.insert(local, s).is_none());

        assert!(conns.get(&local).is_some());
        assert!(conns.get(&other).is_none());

        assert!(conns.remove(&local).is_some());
    }
}
