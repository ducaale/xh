use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::process::Command;

fn get_command() -> Command {
    let mut cmd = Command::cargo_bin("xh").expect("binary should be present");
    cmd.env("XH_TEST_MODE", "1");
    cmd
}

#[test]
fn basic_post() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("post")
        .arg("httpbin.org/post")
        .arg("name=ali");

    cmd.assert().stdout(indoc! {r#"
        POST /post HTTP/1.1
        accept: application/json, */*
        accept-encoding: gzip, deflate
        connection: keep-alive
        content-length: 14
        content-type: application/json
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

        {
            "name": "ali"
        }

    "#});

    Ok(())
}

#[test]
fn basic_get() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("get")
        .arg("httpbin.org/get");

    cmd.assert().stdout(indoc! {r#"
        GET /get HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        connection: keep-alive
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    Ok(())
}

#[test]
fn basic_head() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("head")
        .arg("httpbin.org/get");

    cmd.assert().stdout(indoc! {r#"
        HEAD /get HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        connection: keep-alive
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    Ok(())
}

#[test]
fn basic_options() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("options")
        .arg("httpbin.org/json");

    // Verify that the response is ok and contains an 'allow' header.
    cmd.assert()
        .stdout(predicate::str::contains("HTTP/1.1 200 OK"));
    cmd.assert().stdout(predicate::str::contains("allow:"));

    Ok(())
}

#[test]
fn multiline_value() {
    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("--form")
        .arg("post")
        .arg("httpbin.org/post")
        .arg("foo=bar\nbaz");

    cmd.assert().stdout(indoc! {r#"
        POST /post HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        connection: keep-alive
        content-length: 13
        content-type: application/x-www-form-urlencoded
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

        foo=bar%0Abaz

    "#});
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
        error: Invalid value for '--proxy <<PROTOCOL>:<PROXY_URL>>...': error: Unknown protocol to set a proxy for: invalid

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
        error: Invalid value for '--proxy <<PROTOCOL>:<PROXY_URL>>...': error: Invalid proxy URL '127.0.0.1:8000' for protocol 'invalid': relative URL without a base

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
