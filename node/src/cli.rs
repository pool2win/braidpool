use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Cli {
    /// Braid data directory
    #[arg(long, default_value = "~/.braidpool/")]
    pub datadir: PathBuf,

    /// Bind to a given address and always listen on it
    #[arg(long, default_value = "0.0.0.0:25188")]
    pub bind: String,

    /// Add a peer to connect to and attempt to keep the connection open. This option can be
    /// specified multiple times
    #[arg(long)]
    pub addpeer: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_build_struct_for_parsing_cli_options() {
        let _ = env_logger::try_init();
        let options = vec!["node", "--bind=localhost:25188", "--addpeer=1.2.3.4:8080"];
        let args = Cli::try_parse_from(options).expect("Error parsing options");
        assert_eq!(args.bind, "localhost:25188");
        assert_eq!(args.addpeer, Some(vec!["1.2.3.4:8080".to_string()]));
    }
}
