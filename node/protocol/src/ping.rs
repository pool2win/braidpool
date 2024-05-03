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

use std::net::SocketAddr;

use super::{Message, ProtocolMessage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PingMessage {
    pub message: String,
}

impl ProtocolMessage for PingMessage {
    fn start(_: &SocketAddr) -> Option<Message> {
        Some(Message::Ping(PingMessage {
            message: String::from("ping"),
        }))
    }

    fn response_for_received(&self) -> Result<Option<Message>, String> {
        log::info!("Received {:?}", self.message);
        if self.message == "ping" {
            Ok(Some(Message::Ping(PingMessage {
                message: String::from("pong"),
            })))
        } else {
            Ok(None)
        }
    }
}
