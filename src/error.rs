use std::fmt;
use std::io;

use arrow_schema::ArrowError;
use parquet::errors::ParquetError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Arrow(ArrowError),
    Io(io::Error),
    Json(serde_json::Error),
    Parquet(ParquetError),
    Unsupported(String),
}

impl Error {
    pub fn is_broken_pipe(&self) -> bool {
        match self {
            Self::Io(err) => err.kind() == io::ErrorKind::BrokenPipe,
            Self::Json(err) => err.io_error_kind() == Some(io::ErrorKind::BrokenPipe),
            _ => false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::Parquet(err) => write!(f, "{err}"),
            Self::Unsupported(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ArrowError> for Error {
    fn from(value: ArrowError) -> Self {
        Self::Arrow(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParquetError> for Error {
    fn from(value: ParquetError) -> Self {
        Self::Parquet(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
