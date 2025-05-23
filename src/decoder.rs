use std::cell::Cell;
use std::io::{self, Read};
use std::rc::Rc;
use std::str::FromStr;

use brotli::Decompressor as BrotliDecoder;
use flate2::read::{GzDecoder, ZlibDecoder};
use reqwest::header::{HeaderMap, CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};
use ruzstd::{FrameDecoder, StreamingDecoder as ZstdDecoder};

#[derive(Debug, Clone, Copy)]
pub enum CompressionType {
    Gzip,
    Deflate,
    Brotli,
    Zstd,
}

impl FromStr for CompressionType {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> anyhow::Result<CompressionType> {
        match value {
            // RFC 2616 section 3.5:
            //   For compatibility with previous implementations of HTTP,
            //   applications SHOULD consider "x-gzip" and "x-compress" to be
            //   equivalent to "gzip" and "compress" respectively.
            "gzip" | "x-gzip" => Ok(CompressionType::Gzip),
            "deflate" => Ok(CompressionType::Deflate),
            "br" => Ok(CompressionType::Brotli),
            "zstd" => Ok(CompressionType::Zstd),
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

/// A wrapper that checks whether an error is an I/O error or a decoding error.
///
/// The main purpose of this is to suppress decoding errors that happen because
/// of an empty input. This is behavior we inherited from HTTPie.
///
/// It's load-bearing in the case of HEAD requests, where responses don't have a
/// body but may declare a Content-Encoding.
///
/// We also treat other empty response bodies like this, regardless of the request
/// method. This matches all the user agents I tried (reqwest, requests/HTTPie, curl,
/// wget, Firefox, Chromium) but I don't know if it's prescribed by any RFC.
///
/// As a side benefit we make I/O errors more focused by stripping decoding errors.
///
/// The reader is structured like this:
///
///      OuterReader ───────┐
///   compression codec     ├── [Status]
///     [InnerReader] ──────┘
///    underlying I/O
///
/// The shared Status object is used to communicate.
struct OuterReader<'a> {
    decoder: Box<dyn Read + 'a>,
    status: Option<Rc<Status>>,
}

struct Status {
    has_read_data: Cell<bool>,
    read_error: Cell<Option<io::Error>>,
    error_msg: &'static str,
}

impl Read for OuterReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.decoder.read(buf) {
            Ok(n) => Ok(n),
            Err(err) => {
                let Some(ref status) = self.status else {
                    // No decoder, pass on as is
                    return Err(err);
                };
                match status.read_error.take() {
                    // If an I/O error happened, return that.
                    Some(read_error) => Err(read_error),
                    // If the input was empty, ignore the decoder error.
                    None if !status.has_read_data.get() => Ok(0),
                    // Otherwise, decorate the decoder error with a message.
                    None => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        DecodeError {
                            msg: status.error_msg,
                            err,
                        },
                    )),
                }
            }
        }
    }
}

struct InnerReader<R: Read> {
    reader: R,
    status: Rc<Status>,
}

impl<R: Read> Read for InnerReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.status.read_error.set(None);
        match self.reader.read(buf) {
            Ok(0) => Ok(0),
            Ok(len) => {
                self.status.has_read_data.set(true);
                Ok(len)
            }
            Err(err) => {
                // Store the real error and return a placeholder.
                // The placeholder is intercepted and replaced by the real error
                // before leaving this module.
                // We store the whole error instead of setting a flag because of zstd:
                // - ZstdDecoder::new() fails with a custom error type and it's hard
                //   to extract the underlying io::Error
                // - ZstdDecoder::read() (unlike the other decoders) wraps custom errors
                //   around the underlying io::Error
                let msg = err.to_string();
                let kind = err.kind();
                self.status.read_error.set(Some(err));
                Err(io::Error::new(kind, msg))
            }
        }
    }
}

#[derive(Debug)]
struct DecodeError {
    msg: &'static str,
    err: io::Error,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.err)
    }
}

pub fn decompress(
    reader: &mut impl Read,
    compression_type: Option<CompressionType>,
) -> impl Read + '_ {
    let Some(compression_type) = compression_type else {
        return OuterReader {
            decoder: Box::new(reader),
            status: None,
        };
    };

    let status = Rc::new(Status {
        has_read_data: Cell::new(false),
        read_error: Cell::new(None),
        error_msg: match compression_type {
            CompressionType::Gzip => "error decoding gzip response body",
            CompressionType::Deflate => "error decoding deflate response body",
            CompressionType::Brotli => "error decoding brotli response body",
            CompressionType::Zstd => "error decoding zstd response body",
        },
    });
    let reader = InnerReader {
        reader,
        status: Rc::clone(&status),
    };
    OuterReader {
        decoder: match compression_type {
            CompressionType::Gzip => Box::new(GzDecoder::new(reader)),
            CompressionType::Deflate => Box::new(ZlibDecoder::new(reader)),
            // 32K is the default buffer size for gzip and deflate
            CompressionType::Brotli => Box::new(BrotliDecoder::new(reader, 32 * 1024)),
            CompressionType::Zstd => Box::new(LazyZstdDecoder::Uninit(Some(reader))),
        },
        status: Some(status),
    }
}

/// [ZstdDecoder] reads from its input during construction.
///
/// We need to delay construction until [Read] so read errors stay read errors.
#[allow(clippy::large_enum_variant)]
enum LazyZstdDecoder<R: Read> {
    Uninit(Option<R>),
    Init(ZstdDecoder<R, FrameDecoder>),
}

impl<R: Read> Read for LazyZstdDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            LazyZstdDecoder::Uninit(reader) => match reader.take() {
                Some(reader) => match ZstdDecoder::new(reader) {
                    Ok(decoder) => {
                        *self = LazyZstdDecoder::Init(decoder);
                        self.read(buf)
                    }
                    Err(err) => Err(io::Error::other(err)),
                },
                // We seem to get here in --stream mode because another layer tries
                // to read again after Ok(0).
                None => Err(io::Error::other("failed to construct ZstdDecoder")),
            },
            LazyZstdDecoder::Init(streaming_decoder) => streaming_decoder.read(buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

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
                    .starts_with("error decoding gzip response body"))
            }
        }
    }

    #[test]
    fn underlying_read_errors_are_not_modified() {
        struct SadReader;
        impl Read for SadReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("oh no!"))
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

    #[test]
    fn interrupts_are_handled_gracefully() {
        struct InterruptedReader {
            step: u8,
        }
        impl Read for InterruptedReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.step += 1;
                match self.step {
                    1 => Read::read(&mut b"abc".as_slice(), buf),
                    2 => Err(io::Error::new(io::ErrorKind::Interrupted, "interrupted")),
                    3 => Read::read(&mut b"def".as_slice(), buf),
                    _ => Ok(0),
                }
            }
        }

        for compression_type in [
            None,
            Some(CompressionType::Brotli),
            Some(CompressionType::Deflate),
            Some(CompressionType::Gzip),
            Some(CompressionType::Zstd),
        ] {
            let mut base_reader = InterruptedReader { step: 0 };
            let mut reader = decompress(&mut base_reader, compression_type);
            let mut buffer = Vec::with_capacity(16);
            let res = reader.read_to_end(&mut buffer);
            if compression_type.is_none() {
                res.unwrap();
                assert_eq!(buffer, b"abcdef");
            } else {
                res.unwrap_err();
            }
        }
    }

    #[test]
    fn empty_inputs_do_not_cause_errors() {
        for compression_type in [
            None,
            Some(CompressionType::Brotli),
            Some(CompressionType::Deflate),
            Some(CompressionType::Gzip),
            Some(CompressionType::Zstd),
        ] {
            let mut input: &[u8] = b"";
            let mut reader = decompress(&mut input, compression_type);
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            assert_eq!(buf, b"");

            // Must accept repeated read attempts after EOF (this happens with --stream)
            for _ in 0..10 {
                reader.read_to_end(&mut buf).unwrap();
                assert_eq!(buf, b"");
            }
        }
    }

    #[test]
    fn read_errors_keep_their_context() {
        #[derive(Debug)]
        struct SpecialErr;
        impl std::fmt::Display for SpecialErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self)
            }
        }
        impl std::error::Error for SpecialErr {}

        struct SadReader;
        impl Read for SadReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::WouldBlock, SpecialErr))
            }
        }

        for compression_type in [
            None,
            Some(CompressionType::Brotli),
            Some(CompressionType::Deflate),
            Some(CompressionType::Gzip),
            Some(CompressionType::Zstd),
        ] {
            let mut input = SadReader;
            let mut reader = decompress(&mut input, compression_type);
            let mut buf = Vec::new();
            let err = reader.read_to_end(&mut buf).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
            err.get_ref().unwrap().downcast_ref::<SpecialErr>().unwrap();
        }
    }

    #[test]
    fn true_decode_errors_are_preserved() {
        for compression_type in [
            CompressionType::Brotli,
            CompressionType::Deflate,
            CompressionType::Gzip,
            CompressionType::Zstd,
        ] {
            let mut input: &[u8] = b"bad";
            let mut reader = decompress(&mut input, Some(compression_type));
            let mut buf = Vec::new();
            let err = reader.read_to_end(&mut buf).unwrap_err();

            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
            let decode_err = err
                .get_ref()
                .unwrap()
                .downcast_ref::<DecodeError>()
                .unwrap();
            let real_err = decode_err.source().unwrap();
            let real_err = real_err.downcast_ref::<io::Error>().unwrap();

            // All four decoders make a different choice here...
            // Still the easiest way to check that we're preserving the error
            let expected_kind = match compression_type {
                CompressionType::Gzip => io::ErrorKind::UnexpectedEof,
                CompressionType::Deflate => io::ErrorKind::InvalidInput,
                CompressionType::Brotli => io::ErrorKind::InvalidData,
                CompressionType::Zstd => io::ErrorKind::Other,
            };
            assert_eq!(real_err.kind(), expected_kind);
        }
    }
}
