use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;

#[derive(Deserialize, Debug, PartialEq)]
pub struct Config {
    pub network: Option<NetworkConfig>,
    pub peer: Option<PeerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            network: Some(NetworkConfig::default()),
            peer: Some(PeerConfig::default()),
        }
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct NetworkConfig {
    pub bind: Option<String>,
    pub port: Option<usize>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            bind: Some("localhost".to_string()),
            port: Some(6680),
        }
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct PeerConfig {
    pub seeds: Option<Vec<String>>,
    pub max_peer_count: Option<usize>,
    pub max_pending_messages: Option<usize>,
    pub max_pending_send_to_all: Option<usize>,
}

impl Default for PeerConfig {
    fn default() -> Self {
        PeerConfig {
            seeds: None,
            max_peer_count: Some(10),
            max_pending_messages: Some(32),
            max_pending_send_to_all: Some(128),
        }
    }
}

pub fn load_config_from_file(path: String) -> Option<Config> {
    match read_file(path) {
        Err(_) => Some(Config::default()),
        Ok(contents) => parse_config_from_string(contents),
    }
}

fn parse_config_from_string(contents: String) -> Option<Config> {
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
    fn it_should_load_default_config_correctly() {
        let _ = env_logger::try_init();
        let conf = Config::default();
        let network = conf.network.unwrap();
        let peer = conf.peer.unwrap();
        assert_eq!(network.bind, Some("localhost".to_string()));
        assert_eq!(network.port, Some(6680));
        assert!(peer.seeds.is_none());
        assert_eq!(peer.max_peer_count, Some(10));
        assert_eq!(peer.max_pending_messages, Some(32));
        assert_eq!(peer.max_pending_send_to_all, Some(128));
    }

    #[test]
    fn it_should_load_empty_config_for_empty_string() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(r#""#.to_string()).unwrap();
        assert!(conf.network.is_none());
        assert!(conf.peer.is_none());
    }

    #[test]
    fn it_should_return_error_for_bad_toml() {
        let _ = env_logger::try_init();
        assert!(parse_config_from_string(r#"abcd"#.to_string()).is_none());
    }

    #[test]
    fn it_should_load_default_for_missing_fields() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
[network]
bind="localhost""#
                .to_string(),
        )
        .unwrap();
        assert!(conf.network.is_some());
        assert_eq!(conf.network.unwrap().bind, Some("localhost".to_string()));
        assert!(conf.peer.is_none());
    }

    #[test]
    fn it_should_load_default_for_missing_fields_in_multiple_sections() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
[network]
bind="localhost"
[peer]
max_peer_count = 100
seeds = ["1.2.3.4:8080"]"#
                .to_string(),
        )
        .unwrap();
        assert!(conf.network.is_some());
        assert_eq!(conf.network.unwrap().bind, Some("localhost".to_string()));
        let peer = conf.peer.unwrap();
        assert_eq!(peer.seeds, Some(vec!["1.2.3.4:8080".to_string()]));
        assert_eq!(peer.max_peer_count.unwrap(), 100);
        assert!(peer.max_pending_messages.is_none());
        assert!(peer.max_pending_send_to_all.is_none());
    }

    #[test]
    fn it_should_load_config_without_errors() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("config.toml".to_string()).unwrap();
        let network = conf.network.unwrap();
        let peer = conf.peer.unwrap();
        assert_eq!(network.bind.unwrap(), "localhost");
        assert_eq!(network.port.unwrap(), 6680);
        assert!(peer.seeds.is_some());
        assert_eq!(peer.seeds.unwrap().len(), 0);
        assert_eq!(peer.max_peer_count.unwrap(), 10);
        assert_eq!(peer.max_pending_messages.unwrap(), 32);
        assert_eq!(peer.max_pending_send_to_all.unwrap(), 128);
    }

    #[test]
    fn it_should_load_default_config_when_file_not_found() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("no_such_file.toml".to_string()).unwrap();
        let network = conf.network.unwrap();
        let peer = conf.peer.unwrap();

        assert_eq!(network.bind.unwrap(), "localhost");
        assert_eq!(network.port.unwrap(), 6680);
        assert!(peer.seeds.is_none());
        assert_eq!(peer.max_peer_count.unwrap(), 10);
        assert_eq!(peer.max_pending_messages.unwrap(), 32);
        assert_eq!(peer.max_pending_send_to_all.unwrap(), 128);
    }
}
