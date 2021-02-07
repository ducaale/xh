use assert_cmd::prelude::*;
use indoc::indoc;
use std::process::Command;

fn get_command() -> Command {
    let mut cmd = Command::cargo_bin("ht").expect("binary should be present");
    cmd.env("HT_TEST_MODE", "1");
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
        user-agent: ht/0.0.0 (test mode)

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
        user-agent: ht/0.0.0 (test mode)

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
        user-agent: ht/0.0.0 (test mode)

    "#});

    Ok(())
}
