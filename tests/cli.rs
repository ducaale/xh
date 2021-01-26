use assert_cmd::prelude::*;
use indoc::indoc;
use std::process::Command;

#[test]
fn basic_post() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ht")?;
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

        {
            "name": "ali"
        }

    "#});

    Ok(())
}
