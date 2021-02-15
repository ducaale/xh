use std::{fs::read_to_string, process::Command};

use assert_cmd::prelude::*;
use httpmock::{Method::*, MockServer};
use indoc::indoc;
use predicate::str::contains;
use predicates::prelude::*;
use serde_json::json;
use tempfile::tempdir;

fn get_base_command() -> Command {
    Command::cargo_bin("xh").expect("binary should be present")
}

/// Sensible default command to test with. use [`get_base_command`] if this
/// setup doesn't apply.
fn get_command() -> Command {
    let mut cmd = get_base_command();
    cmd.env("XH_TEST_MODE", "1");
    cmd.arg("--ignore-stdin");
    cmd
}

#[test]
fn basic_json_post() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .header("Content-Type", "application/json")
            .json_body(json!({"name": "ali"}));
        then.header("Content-Type", "application/json")
            .json_body(json!({"got": "name", "status": "ok"}));
    });

    get_command()
        .arg("--pretty=format")
        .arg("post")
        .arg(server.base_url())
        .arg("name=ali")
        .assert()
        .stdout(indoc! {r#"
        {
            "got": "name",
            "status": "ok"
        }"#});
    mock.assert();
}

#[test]
fn basic_get() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET);
        then.body("foobar\n");
    });

    get_command()
        .arg("get")
        .arg(server.base_url())
        .assert()
        .stdout("foobar\n");
    mock.assert();
}

#[test]
fn basic_head() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(HEAD);
    });

    get_command().arg("head").arg(server.base_url()).assert();
    mock.assert();
}

#[test]
fn basic_options() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(OPTIONS);
        then.header("Allow", "GET, HEAD, OPTIONS");
    });

    get_command()
        .arg("-h")
        .arg("options")
        .arg(server.base_url())
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"))
        .stdout(contains("allow:"));
    mock.assert();
}

#[test]
fn multiline_value() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(POST).body("foo=bar%0Abaz");
    });

    get_command()
        .arg("--form")
        .arg("post")
        .arg(server.base_url())
        .arg("foo=bar\nbaz")
        .assert();
    mock.assert();
}

#[test]
fn header() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("X-Foo", "Bar");
    });
    get_command()
        .arg(server.base_url())
        .arg("x-foo:Bar")
        .assert();
    mock.assert();
}

#[test]
fn query_param() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.query_param("foo", "bar");
    });
    get_command()
        .arg(server.base_url())
        .arg("foo==bar")
        .assert();
    mock.assert();
}

#[test]
fn json_param() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.json_body(json!({"foo": [1, 2, 3]}));
    });
    get_command()
        .arg(server.base_url())
        .arg("foo:=[1,2,3]")
        .assert();
    mock.assert();
}

#[test]
fn verbose() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.header("Connection", "keep-alive")
            .header("Content-Type", "application/json")
            .header("Content-Length", "9")
            .header("User-Agent", "xh/0.0.0 (test mode)")
            .json_body(json!({"x": "y"}));
        then.body("a body")
            .header("date", "N/A")
            .header("X-Foo", "Bar");
    });
    get_command()
        .arg("--verbose")
        .arg(server.base_url())
        .arg("x=y")
        .assert()
        .stdout(indoc! {r#"
        POST / HTTP/1.1
        accept-encoding: gzip, deflate
        connection: keep-alive
        user-agent: xh/0.0.0 (test mode)
        accept: application/json, */*
        content-type: application/json
        content-length: 9
        host: http.mock

        {"x":"y"}

        HTTP/1.1 200 OK
        x-foo: Bar
        date: N/A
        content-length: 6

        a body"#});
    mock.assert();
}

#[test]
fn download() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.body("file contents\n");
    });

    let outfile = dir.path().join("outfile");
    get_command()
        .arg("--download")
        .arg("--output")
        .arg(&outfile)
        .arg(server.base_url())
        .assert();
    mock.assert();
    assert_eq!(read_to_string(&outfile).unwrap(), "file contents\n");
}

#[test]
fn proxy_invalid_protocol() {
    let mut cmd = get_command();
    cmd.arg("--offline")
        .arg("--pretty=format")
        .arg("--proxy=invalid:http://127.0.0.1:8000")
        .arg("GET")
        .arg("http://httpbin.org/get");

    cmd.assert().stderr(indoc! {r#"
        error: Invalid value for '--proxy <PROTOCOL:PROXY_URL>...': error: Unknown protocol to set a proxy for: invalid

    "#});
}

#[test]
fn proxy_invalid_proxy_url() {
    let mut cmd = get_command();
    cmd.arg("--offline")
        .arg("--pretty=format")
        .arg("--proxy=invalid:127.0.0.1:8000")
        .arg("GET")
        .arg("http://httpbin.org/get");

    cmd.assert().stderr(indoc! {r#"
        error: Invalid value for '--proxy <PROTOCOL:PROXY_URL>...': error: Invalid proxy URL '127.0.0.1:8000' for protocol 'invalid': relative URL without a base

    "#});
}

#[test]
fn proxy_multiple_valid_proxies() {
    let mut cmd = get_command();
    cmd.arg("--offline")
        .arg("--pretty=format")
        .arg("--proxy=http:https://127.0.0.1:8000")
        .arg("--proxy=https:socks5://127.0.0.1:8000")
        .arg("--proxy=all:http://127.0.0.1:8000")
        .arg("GET")
        .arg("http://httpbin.org/get");

    cmd.assert().success();
}

// TODO: test implicit download filenames
// For this we have to pretend the output is a tty
// This intersects with both #41 and #59