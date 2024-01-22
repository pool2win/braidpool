use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;

#[derive(Default, Deserialize, Debug, PartialEq)]
pub struct Config {
    network: NetworkConfig,
    peer: PeerConfig,
}

#[derive(Default, Deserialize, Debug, PartialEq)]
pub struct NetworkConfig {
    bind: String,
    port: i16,
}

#[derive(Default, Deserialize, Debug, PartialEq)]
pub struct PeerConfig {
    pub seeds: Vec<String>,
    pub max_peer_count: i16,
    pub max_pending_messages: i16,
}

pub fn load_config_from_file(path: String) -> Option<Config> {
    let contents = read_file(path).unwrap();
    let config = toml::from_str(&contents);
    match config {
        Ok(config) => config,
        Err(e) => {
            log::info!("Error parsing config file {:?}", e);
            None
        }
    }
}

fn read_file(path: String) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_load_config_without_errors() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("config.toml".to_string()).unwrap();
        assert_eq!(conf.network.bind, "localhost");
        assert_eq!(conf.network.port, 6680);
        assert_eq!(conf.peer.seeds, vec!["localhost:6681"]);
        assert_eq!(conf.peer.max_peer_count, 10);
        assert_eq!(conf.peer.max_pending_messages, 32);
    }

    // #[test]
    // fn it_should_load_config_without_errors() {
    //     let _ = env_logger::try_init();
    //     let conf = load_config_from_file("config.toml".to_string()).unwrap();
    //     assert_eq!(conf.network.bind, "localhost");
    //     assert_eq!(conf.network.port, 6680);
    // }
}
