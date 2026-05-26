use std::fs::File;
use std::path::Path;

use crate::error::{Error, Result};

pub struct Source {
    pub file: File,
}

pub fn open(path: &Path) -> Result<Source> {
    if path == Path::new("-") {
        return Err(Error::Unsupported(
            "stdin is not supported in v1; provide a local Parquet file path".to_string(),
        ));
    }

    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(Error::Unsupported(
            "directories are not supported".to_string(),
        ));
    }

    Ok(Source {
        file: File::open(path)?,
    })
}
