use serde::{Deserialize, Serialize};

/// Messages sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Request to participate in the room under the given nickname.
    Join { nickname: String },
    /// A chat message to broadcast to the room.
    Chat { content: String },
}

/// Messages sent from the server to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Sent once a client has successfully joined the room.
    Welcome {
        nickname: String,
        users: Vec<String>,
    },
    /// Another participant joined the room.
    Joined { nickname: String },
    /// A participant left the room.
    Left { nickname: String },
    /// A chat message broadcast to the room.
    Chat {
        nickname: String,
        content: String,
        timestamp: String,
    },
    /// A recoverable error, usually sent to a single client.
    Error { message: String },
}

impl ClientMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

impl ServerMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_round_trips() {
        let message = ClientMessage::Chat {
            content: "hello".to_string(),
        };

        let json = message.to_json().unwrap();
        assert_eq!(ClientMessage::from_json(&json).unwrap(), message);
    }

    #[test]
    fn server_message_round_trips() {
        let message = ServerMessage::Chat {
            nickname: "alice".to_string(),
            content: "hi there".to_string(),
            timestamp: "12:00:00".to_string(),
        };

        let json = message.to_json().unwrap();
        assert_eq!(ServerMessage::from_json(&json).unwrap(), message);
    }

    #[test]
    fn messages_are_tagged() {
        let json = ClientMessage::Join {
            nickname: "bob".to_string(),
        }
        .to_json()
        .unwrap();

        assert!(json.contains("\"type\":\"join\""));
    }
}
