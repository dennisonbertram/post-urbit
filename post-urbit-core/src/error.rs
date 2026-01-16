use std::fmt;

#[derive(Debug, Clone)]
pub enum PostUrbitError {
    InvalidInput(&'static str),
    InvalidEncoding(&'static str),
    Crypto(&'static str),
    Io(String),
}

impl fmt::Display for PostUrbitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostUrbitError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            PostUrbitError::InvalidEncoding(msg) => write!(f, "invalid encoding: {msg}"),
            PostUrbitError::Crypto(msg) => write!(f, "crypto error: {msg}"),
            PostUrbitError::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for PostUrbitError {}

impl From<std::io::Error> for PostUrbitError {
    fn from(err: std::io::Error) -> Self {
        PostUrbitError::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PostUrbitError>;
