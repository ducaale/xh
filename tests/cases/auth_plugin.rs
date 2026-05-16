use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use indoc::indoc;

use crate::prelude::*;

fn path_with_plugins_dir() -> OsString {
    let path = env::var_os("PATH").expect("PATH var missing");
    env::join_paths(env::split_paths(&path).chain([PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plugins"
    ))]))
    .unwrap()
}

#[test]
fn hostname_specific_auth() {
    get_command()
        .env("PATH", path_with_plugins_dir())
        .args(["example.com", "--offline", "--auth-type=plugin:token"])
        .assert()
        .stdout(indoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)
            X-Token: 42

        "#});

    get_command()
        .env("PATH", path_with_plugins_dir())
        .args(["example.org", "--offline", "--auth-type=plugin:token"])
        .assert()
        .stdout(indoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

        "#});
}

#[test]
fn generate_signature_from_body() {
    get_command()
        .env("PATH", path_with_plugins_dir())
        .args([
            ":",
            "--offline",
            "--auth-type=plugin:hmac",
            "--auth=123456",
            "hello=world",
        ])
        .assert()
        .stdout(indoc! {r#"
            POST / HTTP/1.1
            Accept: application/json, */*;q=0.5
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Content-Length: 17
            Content-Type: application/json
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)
            X-Signature: af96906743949248f47e5cc5f5bab43b9e59f624740c975d40733822b8b1452f

            {
                "hello": "world"
            }



        "#});
}
