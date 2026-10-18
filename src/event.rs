use crate::protocol::ServerMessage;

/// Events delivered to the application from the networking layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// A protocol message received from the server.
    Server(ServerMessage),
    /// The connection to the server ended.
    Disconnected { reason: String },
    /// A non-fatal error occurred while communicating.
    Error(String),
}
