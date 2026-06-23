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
