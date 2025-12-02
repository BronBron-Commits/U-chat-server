use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    Login {
        username: String,
        password: String,
    },

    SendMessage {
        content: String,
    },

    // NEW — send image/video/file
    SendMedia {
        kind: String,   // "image" / "video" / "file"
        url: String,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerEvent {
    LoginOk {
        token: String,
    },

    Error {
        details: String,
    },

    MessageBroadcast {
        from: String,
        content: String,
    },

    // NEW — broadcast typed media instead of raw strings
    MediaBroadcast {
        from: String,
        kind: String,
        url: String,
    }
}
