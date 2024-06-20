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
