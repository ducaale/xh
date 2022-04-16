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

enum Decoder<R: Read> {
    PlainText(R),
    Gzip(GzDecoder<R>),
    Deflate(ZlibDecoder<R>),
    Brotli(BrotliDecoder<R>),
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Decoder::PlainText(decoder) => decoder.read(buf),
            Decoder::Gzip(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("error decoding gzip response body: {}", e),
                )
            }),
            Decoder::Deflate(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("error decoding deflate response body: {}", e),
                )
            }),
            Decoder::Brotli(decoder) => decoder.read(buf).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("error decoding brotli response body: {}", e),
                )
            }),
        }
    }
}

pub fn decompress(
    reader: &mut impl Read,
    compression_type: Option<CompressionType>,
) -> impl Read + '_ {
    match compression_type {
        Some(CompressionType::Gzip) => Decoder::Gzip(GzDecoder::new(reader)),
        Some(CompressionType::Deflate) => Decoder::Deflate(ZlibDecoder::new(reader)),
        Some(CompressionType::Brotli) => Decoder::Brotli(BrotliDecoder::new(reader, 4096)),
        None => Decoder::PlainText(reader),
    }
}
