use assert_cmd::prelude::*;
use blake2::{Blake2b, Digest};
use indoc::indoc;
use predicates::prelude::*;
use std::fs;
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

fn delete_session_file(identifier: String) -> std::io::Result<()> {
    // Clear session file
    let mut config_dir = match dirs::config_dir() {
        None => panic!("couldn't get config directory"),
        Some(dir) => dir,
    };
    config_dir.push("ht");
    config_dir.push("sessions");
    config_dir.push("httpbin.org");
    let hash = Blake2b::new().chain(identifier).finalize();
    config_dir.push(format!("{:x}", hash).get(0..10).unwrap().to_string());
    config_dir.set_extension("json");
    match fs::remove_file(config_dir) {
        Err(why) => {
            if why.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(why)
            }
        }
        Ok(()) => Ok(()),
    }
}

#[test]
fn session_bearer() -> Result<(), Box<dyn std::error::Error>> {
    match delete_session_file("test_bearer".to_string()) {
        Err(why) => panic!("failed to remove session file : {}", why),
        Ok(()) => (),
    }

    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("--session=test_bearer")
        .arg("--auth-type=Bearer")
        .arg("--auth=token")
        .arg("get")
        .arg("httpbin.org/bearer")
        .arg("foo:bar");

    cmd.assert().stdout(indoc! {r#"
        GET /bearer HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        authorization: Bearer token
        connection: keep-alive
        foo: bar
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("--session=test_bearer")
        .arg("get")
        .arg("httpbin.org/bearer")
        .arg("foooo:baroo");

    cmd.assert().stdout(indoc! {r#"
        GET /bearer HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        authorization: Bearer token
        connection: keep-alive
        foo: bar
        foooo: baroo
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    Ok(())
}

#[test]
fn session_basic() -> Result<(), Box<dyn std::error::Error>> {
    match delete_session_file("test_basic".to_string()) {
        Err(why) => panic!("failed to remove session file : {}", why),
        Ok(()) => (),
    }

    let mut cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("--session=test_basic")
        .arg("--auth-type=Basic")
        .arg("--auth=me:pass")
        .arg("get")
        .arg("httpbin.org/basic-auth/me/pass")
        .arg("foo:bar");

    cmd.assert().stdout(indoc! {r#"
        GET /basic-auth/me/pass HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        authorization: Basic bWU6cGFzcw==
        connection: keep-alive
        foo: bar
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    cmd = get_command();
    cmd.arg("-v")
        .arg("--offline")
        .arg("--ignore-stdin")
        .arg("--pretty=format")
        .arg("--session=test_basic")
        .arg("get")
        .arg("httpbin.org/basic-auth/me/pass")
        .arg("foooo:baroo");

    cmd.assert().stdout(indoc! {r#"
        GET /basic-auth/me/pass HTTP/1.1
        accept: */*
        accept-encoding: gzip, deflate
        authorization: Basic bWU6cGFzcw==
        connection: keep-alive
        foo: bar
        foooo: baroo
        host: httpbin.org
        user-agent: xh/0.0.0 (test mode)

    "#});

    Ok(())
}

#[test]
fn multiline_value() -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}
