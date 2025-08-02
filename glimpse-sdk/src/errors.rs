use std::{error::Error, fmt::Display};

#[derive(Debug, Clone)]
pub enum GlimpseError {
    SocketError(String),
    Custom(String),
    SocketBindError(String),
}

impl Display for GlimpseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlimpseError::SocketError(msg) => write!(f, "Socket error: {}", msg),
            GlimpseError::Custom(msg) => write!(f, "Custom error: {}", msg),
            GlimpseError::SocketBindError(msg) => write!(f, "Socket bind error: {}", msg),
        }
    }
}

impl Error for GlimpseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}
