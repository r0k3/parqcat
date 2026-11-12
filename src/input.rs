use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use tempfile::NamedTempFile;

use crate::error::{Error, Result};

pub struct Source {
    pub file: File,
    // Holds the spool until the reader finishes. Dropping it deletes the file.
    _spool: Option<NamedTempFile>,
}

pub fn open(path: &Path) -> Result<Source> {
    if path == Path::new("-") {
        let stdin = io::stdin();
        let is_terminal = stdin.is_terminal();
        return open_reader(stdin.lock(), is_terminal);
    }

    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(Error::Unsupported(
            "directories are not supported".to_string(),
        ));
    }

    Ok(Source {
        file: File::open(path)?,
        _spool: None,
    })
}

fn open_reader(mut reader: impl Read, is_terminal: bool) -> Result<Source> {
    if is_terminal {
        return Err(Error::Unsupported(
            "stdin is a terminal; pipe Parquet bytes or pass a file path".to_string(),
        ));
    }

    // The Parquet footer is at the end of the file, and the reader seeks.
    // A pipe cannot seek, so copy the stream to a temporary file first.
    let mut spool = NamedTempFile::new()?;
    io::copy(&mut reader, &mut spool)?;
    spool.flush()?;
    let file = spool.reopen()?;
    Ok(Source {
        file,
        _spool: Some(spool),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use super::open_reader;

    struct Boom;

    impl Read for Boom {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            panic!("read stdin from a terminal");
        }
    }

    #[test]
    fn terminal_stdin_errors_without_reading() {
        match open_reader(Boom, true) {
            Err(err) => assert!(err.to_string().contains("stdin is a terminal")),
            Ok(_) => panic!("terminal stdin was accepted"),
        }
    }

    #[test]
    fn spooled_stdin_is_readable_from_the_start() {
        let mut source = open_reader(io::Cursor::new(b"PAR1-bytes"), false).unwrap();
        let mut buf = Vec::new();
        source.file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"PAR1-bytes");
    }
}
