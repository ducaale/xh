use hyper::header::HeaderValue;
use predicates::str::contains;

use crate::prelude::*;

#[test]
fn logs_are_printed_in_debug_mode() {
    get_command()
        .arg("--debug")
        .arg("--offline")
        .arg(":")
        .env_remove("RUST_LOG")
        .assert()
        .stderr(contains("DEBUG xh] Cli {"))
        .success();
}

#[test]
fn logs_are_not_printed_outside_debug_mode() {
    get_command()
        .arg("--offline")
        .arg(":")
        .env_remove("RUST_LOG")
        .assert()
        .stderr("")
        .success();
}

#[test]
fn backtrace_is_printed_in_debug_mode() {
    let mut server = server::http(|_req| async move {
        panic!("test crash");
    });
    server.disable_hit_checks();
    get_command()
        .arg("--debug")
        .arg(server.base_url())
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .assert()
        .stderr(contains("Stack backtrace:"))
        .failure();
}

#[test]
fn backtrace_is_not_printed_outside_debug_mode() {
    let mut server = server::http(|_req| async move {
        panic!("test crash");
    });
    server.disable_hit_checks();
    let cmd = get_command()
        .arg(server.base_url())
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .assert()
        .failure();
    assert!(!std::str::from_utf8(&cmd.get_output().stderr)
        .unwrap()
        .contains("Stack backtrace:"));
}

#[test]
fn checked_status_is_printed_with_single_quiet() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(404)
            .body("".into())
            .unwrap()
    });

    get_command()
        .args(["--quiet", "--check-status", &server.base_url()])
        .assert()
        .code(4)
        .stdout("")
        .stderr("xh: warning: HTTP 404 Not Found\n");
}

#[test]
fn checked_status_is_not_printed_with_double_quiet() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(404)
            .body("".into())
            .unwrap()
    });

    get_command()
        .args(["--quiet", "--quiet", "--check-status", &server.base_url()])
        .assert()
        .code(4)
        .stdout("")
        .stderr("");
}

#[test]
fn warning_for_invalid_redirect() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(302)
            .header("location", "//")
            .body("".into())
            .unwrap()
    });

    get_command()
        .args(["--follow", &server.base_url()])
        .assert()
        .stderr("xh: warning: Redirect to invalid URL: \"//\"\n");
}

#[test]
fn warning_for_non_utf8_redirect() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(302)
            .header("location", HeaderValue::from_bytes(b"\xFF").unwrap())
            .body("".into())
            .unwrap()
    });

    get_command()
        .args(["--follow", &server.base_url()])
        .assert()
        .stderr("xh: warning: Redirect to invalid URL: \"\\xff\"\n");
}

/// This test should fail if rustls's version gets out of sync in Cargo.toml.
#[cfg(feature = "rustls")]
#[test]
fn rustls_emits_logs() {
    let mut server = server::http(|_req| async move {
        unreachable!();
    });
    server.disable_hit_checks();
    let cmd = get_command()
        .arg("--debug")
        .arg(server.base_url().replace("http://", "https://"))
        .env_remove("RUST_LOG")
        .assert()
        .failure();

    assert!(std::str::from_utf8(&cmd.get_output().stderr)
        .unwrap()
        .contains("rustls::"));
}
