// Copyright 2024 Braidpool Developers

// This file is part of Braidpool

// Braidpool is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Braidpool is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Braidpool. If not, see <https://www.gnu.org/licenses/>.

use super::{Message, ProtocolMessage};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::SystemTime};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatMessage {
    pub from: String,
    pub time: SystemTime,
}

impl ProtocolMessage for HeartbeatMessage {
    fn start(addr: &SocketAddr) -> Option<Message> {
        Some(Message::Heartbeat(HeartbeatMessage {
            from: addr.to_string(),
            time: SystemTime::now(),
        }))
    }

    fn response_for_received(&self) -> Result<Option<Message>, String> {
        log::info!("Received {:?}", self);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::str::FromStr;

    use crate::{HeartbeatMessage, Message, ProtocolMessage};

    #[test]
    fn it_matches_start_message_for_handshake() {
        let addr = SocketAddr::from_str("127.0.0.1:6680").unwrap();
        if let Some(Message::Heartbeat(start_message)) = HeartbeatMessage::start(&addr) {
            assert_eq!(start_message.from, String::from("127.0.0.1:6680"))
        }
    }

    #[test]
    fn it_matches_response_message_for_correct_handshake_start() {
        let addr = SocketAddr::from_str("127.0.0.1:6680").unwrap();
        let start_message = HeartbeatMessage::start(&addr).unwrap();
        let response = start_message.response_for_received().unwrap();
        assert_eq!(response, None);
    }
}
