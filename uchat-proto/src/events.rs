use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    Login { username: String, password: String },
    SendMessage { content: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerEvent {
    LoginOk { token: String },
    MessageBroadcast { from: String, content: String },
    Error { details: String },
}
