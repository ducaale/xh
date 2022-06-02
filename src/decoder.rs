use std::io::{self, Read};
use std::str::FromStr;

use brotli::Decompressor as BrotliDecoder;
use flate2::read::{GzDecoder, ZlibDecoder};
use reqwest::header::{HeaderMap, CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};

#[derive(Debug)]
pub enum CompressionType {
    Gzip,
    Deflate,
    Brotli,
}

impl FromStr for CompressionType {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> anyhow::Result<CompressionType> {
        match value {
            "gzip" => Ok(CompressionType::Gzip),
            "deflate" => Ok(CompressionType::Deflate),
            "br" => Ok(CompressionType::Brotli),
            _ => Err(anyhow::anyhow!("unknown compression type")),
        }
    }
}

// See https://github.com/seanmonstar/reqwest/blob/9bd4e90ec3401c2c5bc435c58954f3d52ab53e99/src/async_impl/decoder.rs#L150
pub fn get_compression_type(headers: &HeaderMap) -> Option<CompressionType> {
    let mut compression_type = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .find_map(|value| value.to_str().ok().and_then(|value| value.parse().ok()));

    if compression_type.is_none() {
        compression_type = headers
            .get_all(TRANSFER_ENCODING)
            .iter()
            .find_map(|value| value.to_str().ok().and_then(|value| value.parse().ok()));
    }

    if compression_type.is_some() {
        if let Some(content_length) = headers.get(CONTENT_LENGTH) {
            if content_length == "0" {
                return None;
            }
        }
    }

    compression_type
}

struct InnerReader<R: Read> {
    reader: R,
    has_read_data: bool,
    has_errored: bool,
}

impl<R: Read> InnerReader<R> {
    fn new(reader: R) -> Self {
        InnerReader {
            reader,
            has_read_data: false,
            has_errored: false,
        }
    }
}

impl<R: Read> Read for InnerReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.reader.read(buf) {
            Ok(0) => Ok(0),
            Ok(len) => {
                self.has_read_data = true;
                Ok(len)
            }
            Err(e) => {
                self.has_errored = true;
                Err(e)
            }
        }
    }
}

enum Decoder<R: Read> {
    PlainText(InnerReader<R>),
    Gzip(GzDecoder<InnerReader<R>>),
    Deflate(ZlibDecoder<InnerReader<R>>),
    Brotli(BrotliDecoder<InnerReader<R>>),
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Decoder::PlainText(decoder) => decoder.read(buf),
            Decoder::Gzip(decoder) => match decoder.read(buf) {
                Ok(n) => Ok(n),
                Err(e) if decoder.get_ref().has_errored => Err(e),
                Err(_) if !decoder.get_ref().has_read_data => Ok(0),
                Err(e) => Err(io::Error::new(
                    e.kind(),
                    format!("error decoding gzip response body: {}", e),
                )),
            },
            Decoder::Deflate(decoder) => match decoder.read(buf) {
                Ok(n) => Ok(n),
                Err(e) if decoder.get_ref().has_errored => Err(e),
                Err(_) if !decoder.get_ref().has_read_data => Ok(0),
                Err(e) => Err(io::Error::new(
                    e.kind(),
                    format!("error decoding deflate response body: {}", e),
                )),
            },
            Decoder::Brotli(decoder) => match decoder.read(buf) {
                Ok(n) => Ok(n),
                Err(e) if decoder.get_ref().has_errored => Err(e),
                Err(_) if !decoder.get_ref().has_read_data => Ok(0),
                Err(e) => Err(io::Error::new(
                    e.kind(),
                    format!("error decoding brotli response body: {}", e),
                )),
            },
        }
    }
}

pub fn decompress(
    reader: &mut impl Read,
    compression_type: Option<CompressionType>,
) -> impl Read + '_ {
    let reader = InnerReader::new(reader);
    match compression_type {
        Some(CompressionType::Gzip) => Decoder::Gzip(GzDecoder::new(reader)),
        Some(CompressionType::Deflate) => Decoder::Deflate(ZlibDecoder::new(reader)),
        Some(CompressionType::Brotli) => Decoder::Brotli(BrotliDecoder::new(reader, 4096)),
        None => Decoder::PlainText(reader),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_errors_are_prepended_with_custom_message() {
        let uncompressed_data = String::from("Hello world");
        let mut uncompressed_data = uncompressed_data.as_bytes();
        let mut reader = decompress(&mut uncompressed_data, Some(CompressionType::Gzip));
        let mut buffer = Vec::new();
        match reader.read_to_end(&mut buffer) {
            Ok(_) => unreachable!("gzip should fail to decompress an uncompressed data"),
            Err(e) => {
                assert!(e
                    .to_string()
                    .starts_with("error decoding gzip response body:"))
            }
        }
    }

    #[test]
    fn underlying_read_errors_are_not_modified() {
        struct SadReader;
        impl Read for SadReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, "oh no!"))
            }
        }

        let mut sad_reader = SadReader;
        let mut reader = decompress(&mut sad_reader, Some(CompressionType::Gzip));
        let mut buffer = Vec::new();
        match reader.read_to_end(&mut buffer) {
            Ok(_) => unreachable!("SadReader should never be read"),
            Err(e) => {
                assert!(e.to_string().starts_with("oh no!"))
            }
        }
    }
}
