use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use flate2::read::GzDecoder;
use tempfile::NamedTempFile;

use crate::error::{Error, Result};

pub struct Source {
    pub file: File,
    #[allow(dead_code)]
    temp_file: Option<NamedTempFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OuterCompression {
    None,
    Gzip,
    Zstd,
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

    let mut file = File::open(path)?;
    let wrapper = detect_outer_compression(path, &mut file)?;
    file.seek(SeekFrom::Start(0))?;

    match wrapper {
        OuterCompression::None => Ok(Source {
            file,
            temp_file: None,
        }),
        OuterCompression::Gzip => materialize(file, decode_gzip),
        OuterCompression::Zstd => materialize(file, decode_zstd),
    }
}

fn detect_outer_compression(path: &Path, file: &mut File) -> Result<OuterCompression> {
    let mut magic = [0u8; 6];
    let read = file.read(&mut magic)?;
    let magic = &magic[..read];

    if magic.starts_with(&[0x1f, 0x8b]) {
        return Ok(OuterCompression::Gzip);
    }
    if magic.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        return Ok(OuterCompression::Zstd);
    }
    if magic.starts_with(b"BZh") {
        return Err(Error::Unsupported(
            "unsupported outer compression wrapper: bzip2".to_string(),
        ));
    }
    if magic.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        return Err(Error::Unsupported(
            "unsupported outer compression wrapper: xz".to_string(),
        ));
    }
    if magic.starts_with(b"PK") {
        return Err(Error::Unsupported(
            "unsupported outer compression wrapper: zip".to_string(),
        ));
    }

    match lower_extension(path).as_deref() {
        Some("gz") => Ok(OuterCompression::Gzip),
        Some("zst" | "zstd") => Ok(OuterCompression::Zstd),
        Some("bz2") => Err(Error::Unsupported(
            "unsupported outer compression wrapper: bzip2".to_string(),
        )),
        Some("xz") => Err(Error::Unsupported(
            "unsupported outer compression wrapper: xz".to_string(),
        )),
        Some("zip") => Err(Error::Unsupported(
            "unsupported outer compression wrapper: zip".to_string(),
        )),
        _ => Ok(OuterCompression::None),
    }
}

fn lower_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
}

fn materialize(
    file: File,
    decode: fn(File, &mut NamedTempFile) -> io::Result<()>,
) -> Result<Source> {
    let mut temp_file = NamedTempFile::new()?;
    decode(file, &mut temp_file)?;
    temp_file.flush()?;
    let materialized = temp_file.reopen()?;

    Ok(Source {
        file: materialized,
        temp_file: Some(temp_file),
    })
}

fn decode_gzip(file: File, output: &mut NamedTempFile) -> io::Result<()> {
    let mut decoder = GzDecoder::new(BufReader::new(file));
    io::copy(&mut decoder, output)?;
    Ok(())
}

fn decode_zstd(file: File, output: &mut NamedTempFile) -> io::Result<()> {
    let mut decoder = zstd::stream::read::Decoder::new(BufReader::new(file))?;
    io::copy(&mut decoder, output)?;
    Ok(())
}
