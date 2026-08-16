use std::env;
use std::ffi::OsString;
use std::fs;
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
fn set_auth_based_on_url() {
    get_command()
        .env("PATH", path_with_plugins_dir())
        .args([
            "example.com",
            "--offline",
            if cfg!(windows) {
                "--auth-type=plugin-token.cmd"
            } else {
                "--auth-type=plugin-token"
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
                "--auth-type=plugin-token.cmd"
            } else {
                "--auth-type=plugin-token"
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
fn can_refer_to_plugin_by_path() {
    get_command()
        .args([
            "example.com",
            "--offline",
            if cfg!(windows) {
                "--auth-type=./tests/fixtures/plugins/xh-plugin-token.cmd"
            } else {
                "--auth-type=./tests/fixtures/plugins/xh-plugin-token"
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
}

#[test]
fn can_parse_error_from_plugin() {
    get_command()
        .args([
            "example.com",
            "--offline",
            if cfg!(windows) {
                "--auth-type=./tests/fixtures/plugins/xh-plugin-token.cmd"
            } else {
                "--auth-type=./tests/fixtures/plugins/xh-plugin-token"
            },
            "--auth=secret-token",
        ])
        .assert()
        .stderr(indoc! {r#"
            xh: error: -a/--auth cannot be used with xh-plugin-token
        "#});
}

#[test]
fn persist_in_session() {
    let mut path_to_session = std::env::temp_dir();
    let file_name = random_string();
    path_to_session.push(file_name);

    get_command()
        .env("PATH", path_with_plugins_dir())
        .args([
            "example.com",
            "--offline",
            if cfg!(windows) {
                "--auth-type=plugin-token.cmd"
            } else {
                "--auth-type=plugin-token"
            },
        ])
        .arg(format!("--session={}", path_to_session.to_string_lossy()))
        .assert()
        .success();

    let session_content = fs::read_to_string(path_to_session).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": {
                "about": "xh session file",
                "xh": "0.0.0"
            },
            "auth": { "type": null, "raw_auth": null },
            "cookies": [],
            "headers": [
                { "name": "x-token", "value": "42" }
            ]
        })
    );
}
