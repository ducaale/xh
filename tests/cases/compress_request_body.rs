use std::{fs::OpenOptions, io::Read as _};

use hyper::header::HeaderValue;

use crate::prelude::*;
use std::io::Write;

fn zlib_decode(bytes: Vec<u8>) -> std::io::Result<String> {
    let mut z = flate2::read::ZlibDecoder::new(&bytes[..]);
    let mut s = String::new();
    z.read_to_string(&mut s)?;
    Ok(s)
}

fn server() -> server::Server {
    server::http(|req| async move {
        match req.uri().path() {
            "/deflate" => {
                assert_eq!(
                    req.headers().get(hyper::header::CONTENT_ENCODING),
                    Some(HeaderValue::from_static("deflate")).as_ref()
                );

                let compressed_body = req.body().await;
                let body = zlib_decode(compressed_body).unwrap();
                hyper::Response::builder()
                    .header("date", "N/A")
                    .header("Content-Type", "text/plain")
                    .body(body.into())
                    .unwrap()
            }
            "/normal" => {
                let body = req.body_as_string().await;
                hyper::Response::builder()
                    .header("date", "N/A")
                    .header("Content-Type", "text/plain")
                    .body(body.into())
                    .unwrap()
            }
            _ => panic!("unknown path"),
        }
    })
}

#[test]
fn compress_request_body_json() {
    let server = server();

    get_command()
        .arg(format!("{}/deflate", server.base_url()))
        .args([
            &format!("key={}", "1".repeat(1000)),
            "-x",
            "-j",
            "--pretty=none",
        ])
        .assert()
        .stdout(indoc::formatdoc! {r#"
            HTTP/1.1 200 OK
            Date: N/A
            Content-Type: text/plain
            Content-Length: 1010

            {{"key":"{c}"}}
        "#, c = "1".repeat(1000),});
}
#[test]
fn compress_request_body_form() {
    let server = server();

    get_command()
        .arg(format!("{}/deflate", server.base_url()))
        .args([
            &format!("key={}", "1".repeat(1000)),
            "-x",
            "-x",
            "-f",
            "--pretty=none",
        ])
        .assert()
        .stdout(indoc::formatdoc! {r#"
            HTTP/1.1 200 OK
            Date: N/A
            Content-Type: text/plain
            Content-Length: 1004

            key={c}
        "#, c = "1".repeat(1000),});
}

#[test]
fn skip_compression_when_compression_ratio_is_negative() {
    let server = server();
    get_command()
        .arg(format!("{}/normal", server.base_url()))
        .args([&format!("key={}", "1"), "-x", "-f", "--pretty=none"])
        .assert()
        .stdout(indoc::formatdoc! {r#"
            HTTP/1.1 200 OK
            Date: N/A
            Content-Type: text/plain
            Content-Length: 5

            key={c}
        "#, c = "1"});
}

#[test]
fn test_compress_force_with_negative_ratio() {
    let server = server();
    get_command()
        .arg(format!("{}/deflate", server.base_url()))
        .args([&format!("key={}", "1"), "-xx", "-f", "--pretty=none"])
        .assert()
        .stdout(indoc::formatdoc! {r#"
            HTTP/1.1 200 OK
            Date: N/A
            Content-Type: text/plain
            Content-Length: 5

            key={c}
        "#, c = "1"});
}

#[test]
fn dont_compress_request_body_if_content_encoding_have_value() {
    let server = server();
    get_command()
        .arg(format!("{}/normal", server.base_url()))
        .args([
            &format!("key={}", "1".repeat(1000)),
            "content-encoding:gzip",
            "-xx",
            "-f",
            "--pretty=none",
        ])
        .assert()
        .stdout(indoc::formatdoc! {r#"
            HTTP/1.1 200 OK
            Date: N/A
            Content-Type: text/plain
            Content-Length: 1004

            key={c}
        "#, c = "1".repeat(1000),});
}

#[test]
fn compress_body_from_file() {
    let server = server::http(|req| async move {
        assert_eq!("Hello world\n", zlib_decode(req.body().await).unwrap());
        hyper::Response::default()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg(server.base_url())
        .arg("-xx")
        .arg(format!("@{}", filename.to_string_lossy()))
        .assert()
        .success();
}

#[test]
fn compress_body_from_file_unless_compress_rate_less_1() {
    let server = server::http(|req| async move {
        assert_eq!("Hello world\n", req.body_as_string().await);
        hyper::Response::default()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg(server.base_url())
        .arg("-x")
        .arg(format!("@{}", filename.to_string_lossy()))
        .assert()
        .success();
}
#[test]
fn test_cannot_combine_compress_with_multipart() {
    get_command()
        .arg(format!("{}/deflate", ""))
        .args(["--multipart", "-x", "a=1"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the argument '--multipart' cannot be used with '--compress...'",
        ));
}
