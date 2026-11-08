use std::io::Write;

use predicates::{prelude::PredicateBooleanExt, str::contains};
use tempfile::NamedTempFile;

use crate::prelude::*;

fn http_file(contents: impl AsRef<[u8]>) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(contents.as_ref()).unwrap();
    file
}

#[test]
fn http_file_replays_request() {
    for newline in ["\n", "\r\n"] {
        let server = server::http(|req| async move {
            assert_eq!(req.method(), "PUT");
            assert_eq!(req.uri(), "/a%20path?q=one");
            assert_eq!(req.headers()["content-type"], "text/plain");
            assert_eq!(req.headers()["x-example"], "value: with colon");
            assert_eq!(req.headers()["x-empty"], "");
            assert_eq!(req.headers()["content-length"], "11");
            assert_eq!(req.body().await, b"one\r\ntwo\n\0\xff");
            hyper::Response::default()
        });
        let mut request = format!(
            "PUT /a path?q=one HTTP/1.1{newline}Host: {}:{}{newline}Content-Type: text/plain{newline}Content-Length: 999{newline}X-Example: value: with colon{newline}X-Empty:{newline}{newline}",
            server.host(), server.port()
        ).into_bytes();
        request.extend_from_slice(b"one\r\ntwo\n\0\xff");
        let file = http_file(request);
        get_command()
            .arg("--http-file")
            .arg(file.path())
            .assert()
            .success();
    }
}

#[test]
fn http_file_cli_headers_override_and_unset() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "GET");
        assert_eq!(req.headers().get_all("authorization").iter().count(), 1);
        assert_eq!(req.headers()["authorization"], "Bearer new");
        assert!(!req.headers().contains_key("x-remove"));
        assert_eq!(req.headers().get_all("x-repeat").iter().count(), 2);
        assert_eq!(req.headers()["x-new"], "added");
        assert_eq!(req.body().await, b"");
        hyper::Response::default()
    });
    let file = http_file(format!(
        "GET / HTTP/1.1\nHost: {}:{}\nAuthorization: Bearer old\nX-Remove: old\nX-Repeat: one\nX-Repeat: two\n\n",
        server.host(),
        server.port()
    ));
    get_command()
        .arg("--http-file")
        .arg(file.path())
        .args(["authorization:Bearer new", "x-remove:", "x-new:added"])
        .assert()
        .success();
}

#[test]
fn http_file_offline_round_trip() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.uri(), "/posts");
        assert_eq!(req.headers()["content-type"], "application/json");
        assert_eq!(req.headers()["authorization"], "Bearer secret");
        let body: serde_json::Value = serde_json::from_slice(&req.body().await).unwrap();
        assert_eq!(body, serde_json::json!({"title":"foo", "userId":1}));
        hyper::Response::default()
    });
    let saved = redirecting_command()
        .env_remove("XH_TEST_MODE")
        .args(["--ignore-stdin", "--offline", "--pretty=none"])
        .arg(server.url("/posts"))
        .args(["title=foo", "userId:=1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let file = http_file(saved);
    get_command()
        .arg("--http-file")
        .arg(file.path())
        .arg("authorization:Bearer secret")
        .assert()
        .success();
}

#[test]
fn http_file_offline_and_scheme() {
    let file = http_file("HEAD /path?q=1 HTTP/2\nHost: example.org\nX-Test: yes\n");
    get_command()
        .args(["--offline", "--https", "--debug", "--http-file"])
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("HEAD /path?q=1 HTTP/2.0"))
        .stdout(contains("x-test: yes"))
        .stderr(contains("Complete URL: https://example.org/path?q=1"));
}

#[test]
fn http_file_does_not_add_json_headers() {
    let file = http_file("POST / HTTP/1.1\nHost: example.org\n\nhello");
    let output = get_command()
        .args(["--offline", "--http-file"])
        .arg(file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("hello"));
    assert!(!output.contains("Content-Type:"));
    assert!(!output.contains("application/json"));
}

#[test]
fn http_file_ignores_stdin() {
    let file = http_file("POST / HTTP/1.1\nHost: example.org\n\nfrom file");
    redirecting_command()
        .args(["--offline", "--http-file"])
        .arg(file.path())
        .write_stdin("from stdin")
        .assert()
        .success()
        .stdout(contains("from file"));
}

#[test]
fn http_file_rejects_malformed_requests() {
    for (request, error) in [
        ("", "request line"),
        ("GET /\nHost: example.org\n", "request line"),
        ("GET / HTTP/9\nHost: example.org\n", "HTTP version"),
        ("GET / HTTP/1.1\n", "Host"),
        ("GET / HTTP/1.1\nHost: example.org\nbroken\n", "header"),
        (
            "GET / HTTP/1.1\nHost: example.org\nBad Name: value\n",
            "header",
        ),
        (
            "GET / HTTP/1.1\nHost: example.org\nHost: other.org\n",
            "Host",
        ),
        ("GET / HTTP/1.1\nHost: example.org/path\n", "Host"),
        (
            "POST / HTTP/1.1\nHost: example.org\nContent-Type: multipart/form-data; boundary=x\n\n--x",
            "Multipart",
        ),
        (
            "POST / HTTP/1.1\nHost: example.org\nTransfer-Encoding: chunked\n\n0\r\n\r\n",
            "Transfer-Encoding",
        ),
    ] {
        let file = http_file(request);
        get_command()
            .args(["--offline", "--http-file"])
            .arg(file.path())
            .assert()
            .failure()
            .stderr(contains(error));
    }
}

#[test]
fn http_file_rejects_body_options_and_items() {
    let file = http_file("POST / HTTP/1.1\nHost: example.org\n\nhello");
    for item in [
        "--raw=other",
        "--multipart",
        "--form",
        "--json",
        "key=value",
        "@body.txt",
    ] {
        get_command()
            .args(["--offline", "--http-file"])
            .arg(file.path())
            .arg(item)
            .assert()
            .failure()
            .stderr(contains("cannot be mixed").or(contains("cannot be used")));
    }
}

#[test]
fn http_file_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    get_command()
        .args(["--offline", "--http-file"])
        .arg(dir.path().join("missing.http"))
        .assert()
        .failure()
        .stderr(contains("Failed to read HTTP file"));
}

#[test]
fn http_file_curl_translation() {
    let file = http_file(
        "POST /posts HTTP/1.1\nHost: example.org\nAuthorization: old\nContent-Length: 999\nX-Remove: old\n\na=b&c=d\n",
    );
    get_command()
        .args(["--curl", "--http-file"])
        .arg(file.path())
        .args(["authorization:new", "x-remove:"])
        .assert()
        .success()
        .stdout(contains("http://example.org/posts"))
        .stdout(contains("authorization: new"))
        .stdout(contains("--data-raw"))
        .stdout(contains("a=b&c=d"))
        .stdout(contains("999").not())
        .stdout(contains("old").not());
}

#[test]
fn http_file_version_and_query_overrides() {
    let file = http_file("GET /path?original=one HTTP/2\nHost: example.org\n\n");
    get_command()
        .args(["--offline", "--http-version=1.0", "--http-file"])
        .arg(file.path())
        .arg("added==two")
        .assert()
        .success()
        .stdout(contains("GET /path?original=one&added=two HTTP/1.0"));
}

#[test]
fn http_file_curl_rejects_binary_body() {
    for body in [b"\0".as_slice(), b"\xff".as_slice()] {
        let mut contents = b"POST / HTTP/1.1\nHost: example.org\n\n".to_vec();
        contents.extend_from_slice(body);
        let file = http_file(contents);
        get_command()
            .args(["--curl", "--http-file"])
            .arg(file.path())
            .assert()
            .failure()
            .stderr(contains("Cannot translate a binary HTTP file body to curl"));
    }
}
