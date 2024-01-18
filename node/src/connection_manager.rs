use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::SystemTime;

/// Connection state including connection initialisation time
#[derive(Clone)]
pub struct Metadata {
    pub created_at: SystemTime,
}

type Map = HashMap<SocketAddr, Metadata>;

pub struct ConnectionManager {
    shared_state: Mutex<Map>,
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
            shared_state: Mutex::new(Map::new()),
        }
    }

    /// Insert a new peer's IP address and sender channel
    pub fn insert(&self, addr: SocketAddr, s: Metadata) -> Option<Metadata> {
        let mut state = self.shared_state.lock().unwrap();
        state.insert(addr, s)
    }

    /// Remove a peer's IP address
    pub fn remove(&self, addr: &SocketAddr) -> Option<Metadata> {
        let mut state = self.shared_state.lock().unwrap();
        state.remove(addr)
    }

    /// Find the sender channel for a peer's SocketAddr
    pub fn get(&self, addr: &SocketAddr) -> Option<Metadata> {
        let state = self.shared_state.lock().unwrap();
        state.get(addr).cloned()
    }

    /// Returns the number of connections
    pub fn num_connections(&self) -> usize {
        let state = self.shared_state.lock().unwrap();
        state.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn it_should_create_connections_map() {
        let conns = ConnectionManager::new();
        assert_eq!(conns.shared_state.lock().unwrap().len(), 0);
    }

    #[test]
    pub fn it_should_insert_new_address() {
        let manager = ConnectionManager::new();
        let s = Metadata {
            created_at: SystemTime::now(),
        };
        let local = "127.0.0.1:8080".parse().unwrap();
        let other = "127.0.0.1:8081".parse().unwrap();

        assert!(manager.insert(local, s).is_none());
        assert_eq!(manager.num_connections(), 1);

        assert!(manager.get(&local).is_some());
        assert!(manager.get(&other).is_none());

        assert!(manager.remove(&local).is_some());
        assert_eq!(manager.num_connections(), 0);
    }
}
