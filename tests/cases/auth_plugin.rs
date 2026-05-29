use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use hyper::header::HeaderValue;
use indoc::indoc;
use predicates::str::contains;

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
fn set_auth_based_on_url() {
    get_command()
        .env("PATH", path_with_plugins_dir())
        .args([
            "example.com",
            "--offline",
            if cfg!(windows) {
                "--auth-type=plugin:token.cmd"
            } else {
                "--auth-type=plugin:token"
            },
        ])
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

    // no auth for example.org
    get_command()
        .env("PATH", path_with_plugins_dir())
        .args([
            "example.org",
            "--offline",
            if cfg!(windows) {
                "--auth-type=plugin:token.cmd"
            } else {
                "--auth-type=plugin:token"
            },
        ])
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
            if cfg!(windows) {
                "--auth-type=plugin:hmac.cmd"
            } else {
                "--auth-type=plugin:hmac"
            },
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

#[test]
fn plugin_can_set_state() {
    let server = server::http(|req| async move {
        if req.headers().get("redirect-counter") == Some(&HeaderValue::from_static("5")) {
            return hyper::Response::builder().body("success!".into()).unwrap();
        }
        match req.uri().path() {
            "/page_a" => hyper::Response::builder()
                .status(302)
                .header("location", "/page_a")
                .body("".into())
                .unwrap(),
            "/page_b" => hyper::Response::builder()
                .status(302)
                .header("location", "/page_a")
                .body("".into())
                .unwrap(),
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .env("PATH", path_with_plugins_dir())
        .arg(server.url("/page_a"))
        .arg("--follow")
        .arg(if cfg!(windows) {
            "--auth-type=plugin:redirect-counter.cmd"
        } else {
            "--auth-type=plugin:redirect-counter"
        })
        .assert()
        .stdout(contains("success!"));
}
