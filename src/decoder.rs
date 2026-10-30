use std::cell::Cell;
use std::io::{self, BufRead, BufReader, Read};
use std::rc::Rc;
use std::str::FromStr;

use brotli::Decompressor as BrotliDecoder;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use reqwest::header::{CONTENT_ENCODING, CONTENT_LENGTH, HeaderMap, TRANSFER_ENCODING};
use ruzstd::frame::ReadFrameHeaderError;
use ruzstd::frame_decoder::FrameDecoderError;
use ruzstd::{BlockDecodingStrategy, FrameDecoder};

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
                // We store the whole error instead of setting a flag because ruzstd
                // wraps I/O errors in custom errors during frame initialization and
                // decoding, making the original io::Error hard to recover.
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
            CompressionType::Deflate => Box::new(LazyDeflateDecoder::new(reader)),
            // 32K is the default buffer size for gzip and deflate
            CompressionType::Brotli => Box::new(BrotliDecoder::new(reader, 32 * 1024)),
            CompressionType::Zstd => Box::new(LazyZstdDecoder::new(reader)),
        },
        status: Some(status),
    }
}

/// A deflate decoder that also accepts raw deflate data without the zlib
/// wrapper, which some servers send. Like curl and urllib3, it looks at the
/// first two bytes to tell them apart, and reads nothing until it is read.
enum LazyDeflateDecoder<R: Read> {
    Pending {
        reader: Option<R>,
        head: [u8; 2],
        len: usize,
    },
    Zlib(ZlibDecoder<DeflateInput<R>>),
    Raw(DeflateDecoder<DeflateInput<R>>),
}

type DeflateInput<R> = io::Chain<io::Take<io::Cursor<[u8; 2]>>, R>;

impl<R: Read> LazyDeflateDecoder<R> {
    fn new(reader: R) -> Self {
        LazyDeflateDecoder::Pending {
            reader: Some(reader),
            head: [0; 2],
            len: 0,
        }
    }
}

impl<R: Read> Read for LazyDeflateDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let LazyDeflateDecoder::Pending { reader, head, len } = self {
            let inner = reader.as_mut().expect("reader is only taken once");
            while *len < 2 {
                match inner.read(&mut head[*len..])? {
                    0 => break,
                    n => *len += n,
                }
            }
            // RFC 1950: compression method 8 and a header checksum divisible by 31
            let zlib = *len < 2 || (head[0] & 0x0f == 8 && u16::from_be_bytes(*head) % 31 == 0);
            let input = io::Cursor::new(*head)
                .take(*len as u64)
                .chain(reader.take().unwrap());
            *self = if zlib {
                LazyDeflateDecoder::Zlib(ZlibDecoder::new(input))
            } else {
                LazyDeflateDecoder::Raw(DeflateDecoder::new(input))
            };
        }
        match self {
            LazyDeflateDecoder::Zlib(decoder) => decoder.read(buf),
            LazyDeflateDecoder::Raw(decoder) => decoder.read(buf),
            LazyDeflateDecoder::Pending { .. } => unreachable!(),
        }
    }
}

/// A lazy decoder for a stream containing any number of zstd frames.
///
/// ruzstd's high-level streaming decoder reads during construction and stops
/// after one frame. Using [FrameDecoder] directly lets us defer all reads until
/// [Read] and continue through concatenated and skippable frames.
struct LazyZstdDecoder<R: Read> {
    reader: BufReader<R>,
    decoder: FrameDecoder,
    state: ZstdDecoderState,
}

#[derive(Clone, Copy)]
enum ZstdDecoderState {
    NeedFrame,
    Decoding,
    Finished,
}

impl<R: Read> LazyZstdDecoder<R> {
    fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            decoder: FrameDecoder::new(),
            state: ZstdDecoderState::NeedFrame,
        }
    }
}

impl<R: Read> Read for LazyZstdDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            match self.state {
                ZstdDecoderState::NeedFrame => {
                    if self.reader.fill_buf()?.is_empty() {
                        self.state = ZstdDecoderState::Finished;
                        return Ok(0);
                    }

                    match self.decoder.reset(&mut self.reader) {
                        Ok(()) => self.state = ZstdDecoderState::Decoding,
                        Err(FrameDecoderError::ReadFrameHeaderError(
                            ReadFrameHeaderError::SkipFrame { length, .. },
                        )) => {
                            let length = u64::from(length);
                            let copied = {
                                let mut payload = self.reader.by_ref().take(length);
                                io::copy(&mut payload, &mut io::sink())?
                            };
                            if copied != length {
                                return Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "truncated zstd skippable frame",
                                ));
                            }
                        }
                        Err(err) => return Err(io::Error::other(err)),
                    }
                }
                ZstdDecoderState::Decoding => {
                    while self.decoder.can_collect() < buf.len() && !self.decoder.is_finished() {
                        let additional_bytes = buf.len() - self.decoder.can_collect();
                        self.decoder
                            .decode_blocks(
                                &mut self.reader,
                                BlockDecodingStrategy::UptoBytes(additional_bytes),
                            )
                            .map_err(io::Error::other)?;
                    }

                    let read = self.decoder.read(buf)?;
                    if read != 0 {
                        return Ok(read);
                    }
                    self.state = ZstdDecoderState::NeedFrame;
                }
                ZstdDecoderState::Finished => return Ok(0),
            }
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
                assert!(
                    e.to_string()
                        .starts_with("error decoding gzip response body")
                )
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
    fn deflate_accepts_zlib_and_raw_streams() {
        struct OneByteReader<'a>(&'a [u8]);
        impl Read for OneByteReader<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let len = buf.len().min(self.0.len()).min(1);
                buf[..len].copy_from_slice(&self.0[..len]);
                self.0 = &self.0[len..];
                Ok(len)
            }
        }

        let zlib = include_bytes!("../tests/fixtures/responses/hello_world.zz");
        let raw = &zlib[2..zlib.len() - 4];
        for input in [&zlib[..], raw] {
            let mut output = Vec::new();
            decompress(&mut OneByteReader(input), Some(CompressionType::Deflate))
                .read_to_end(&mut output)
                .unwrap();
            assert_eq!(output, b"Hello world\n");
        }
    }

    #[test]
    fn zstd_decodes_concatenated_and_skippable_frames() {
        let frame = include_bytes!("../tests/fixtures/responses/hello_world.zst");
        let skipped = b"not compressed";
        let mut input = Vec::new();
        input.extend_from_slice(frame);
        input.extend_from_slice(&0x184d_2a50_u32.to_le_bytes());
        input.extend_from_slice(&(skipped.len() as u32).to_le_bytes());
        input.extend_from_slice(skipped);
        input.extend_from_slice(frame);

        let mut input = input.as_slice();
        let mut reader = decompress(&mut input, Some(CompressionType::Zstd));
        let mut output = Vec::new();
        reader.read_to_end(&mut output).unwrap();
        assert_eq!(output, b"Hello world\nHello world\n");

        // Must accept repeated read attempts after the last frame.
        for _ in 0..10 {
            assert_eq!(reader.read(&mut [0]).unwrap(), 0);
        }
    }

    #[test]
    fn zstd_rejects_truncated_following_frames() {
        let frame = include_bytes!("../tests/fixtures/responses/hello_world.zst");
        let truncated_magic_number = [0x28, 0xb5, 0x2f];

        // The second case includes a complete frame and block header, but only
        // the first byte of the block payload.
        for truncated_frame in [truncated_magic_number.as_slice(), &frame[..10]] {
            let mut input = Vec::from(frame.as_slice());
            input.extend_from_slice(truncated_frame);

            let mut input = input.as_slice();
            let mut reader = decompress(&mut input, Some(CompressionType::Zstd));
            let mut output = Vec::new();
            let err = reader.read_to_end(&mut output).unwrap_err();

            assert_eq!(output, b"Hello world\n");
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
            assert_eq!(err.to_string(), "error decoding zstd response body");
        }
    }

    #[test]
    fn zstd_rejects_a_truncated_skippable_frame() {
        let frame = include_bytes!("../tests/fixtures/responses/hello_world.zst");
        let mut input = Vec::from(frame.as_slice());
        input.extend_from_slice(&0x184d_2a50_u32.to_le_bytes());
        input.extend_from_slice(&10_u32.to_le_bytes());
        input.extend_from_slice(b"short");

        let mut input = input.as_slice();
        let mut reader = decompress(&mut input, Some(CompressionType::Zstd));
        let mut output = Vec::new();
        let err = reader.read_to_end(&mut output).unwrap_err();

        assert_eq!(output, b"Hello world\n");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "error decoding zstd response body");
    }

    #[test]
    fn read_errors_keep_their_context() {
        #[derive(Debug)]
        struct SpecialErr;
        impl std::fmt::Display for SpecialErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{self:?}")
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
                // "bad" has no zlib header, so it is decoded as raw deflate
                CompressionType::Deflate => io::ErrorKind::UnexpectedEof,
                CompressionType::Brotli => io::ErrorKind::InvalidData,
                CompressionType::Zstd => io::ErrorKind::Other,
            };
            assert_eq!(real_err.kind(), expected_kind);
        }
    }
}
