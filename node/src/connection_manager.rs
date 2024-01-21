use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::SystemTime;

/// Connection state including connection initialisation time
#[derive(Debug, Copy, Clone)]
pub struct Metadata {
    pub created_at: SystemTime,
}

type Map = HashMap<SocketAddr, Metadata>;

pub struct ConnectionManager {
    capacity: usize,
    shared_state: Mutex<Map>,
}

/// ConnectionManager struct maps a peer's IP address to metadata
/// about the connection.
impl ConnectionManager {
    pub fn new(capacity: usize) -> Self {
        ConnectionManager {
            capacity,
            /// Use a Mutex here as changes to this table will be
            /// infrequent. We can replace with crossbeam SkipMap if
            /// this becomes a performance bottleneck.
            shared_state: Mutex::new(Map::new()),
        }
    }

    /// Insert a new peer's IP address and metadata
    pub fn insert(
        &self,
        addr: SocketAddr,
        s: Metadata,
    ) -> Result<Option<Metadata>, Box<dyn Error>> {
        let mut state = self.shared_state.lock().unwrap();
        if state.len() >= self.capacity {
            return Err("Connection limit reached".into());
        }
        let result = Ok(state.insert(addr, s));
        log::info!(
            "Handling new connection. Number of connections {:?}",
            state.len()
        );
        result
    }

    /// Remove a peer's IP address
    pub fn remove(&self, addr: &SocketAddr) -> Option<Metadata> {
        let mut state = self.shared_state.lock().unwrap();
        state.remove(addr)
    }

    /// Find the metadata for a peer's SocketAddr
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
        let conns = ConnectionManager::new(10);
        assert_eq!(conns.shared_state.lock().unwrap().len(), 0);
    }

    #[test]
    pub fn it_should_insert_new_address() {
        let manager = ConnectionManager::new(10);
        let s = Metadata {
            created_at: SystemTime::now(),
        };
        let local = "127.0.0.1:8080".parse().unwrap();
        let other = "127.0.0.1:8081".parse().unwrap();

        assert!(manager.insert(local, s).unwrap().is_none());
        assert_eq!(manager.num_connections(), 1);

        assert!(manager.get(&local).is_some());
        assert!(manager.get(&other).is_none());

        assert!(manager.remove(&local).is_some());
        assert_eq!(manager.num_connections(), 0);
    }

    #[test]
    pub fn it_should_not_insert_new_address_when_limit_reached() {
        let manager = ConnectionManager::new(1);
        let s = Metadata {
            created_at: SystemTime::now(),
        };
        let local = "127.0.0.1:8080".parse().unwrap();
        let other = "127.0.0.1:8081".parse().unwrap();

        assert!(manager.insert(local, s).unwrap().is_none());
        assert_eq!(manager.num_connections(), 1);

        assert!(manager.insert(other, s).is_err());
        assert_eq!(manager.num_connections(), 1);
    }
}
