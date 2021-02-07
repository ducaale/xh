use assert_cmd::prelude::*;
use indoc::indoc;
use std::fs;
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

#[test]
fn basic_head() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ht")?;
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

        "#});

    Ok(())
}
#[test]
fn basic_get() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ht")?;
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
    
        "#});

    Ok(())
}
fn clear_session_file(name: String) -> std::io::Result<()> {
    // Clear session file
    let mut config_dir = match dirs::config_dir() {
        None => panic!("couldn't get config directory"),
        Some(dir) => dir,
    };
    config_dir.push("ht");
    config_dir.push("sessions");
    config_dir.push("httpbin.org");
    config_dir.push(name);
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
    match clear_session_file("test_bearer".to_string()) {
        Err(why) => panic!("failed to remove session file : {}", why),
        Ok(()) => (),
    }

    let mut cmd = Command::cargo_bin("ht")?;
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

    "#});

    cmd = Command::cargo_bin("ht")?;
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

    "#});

    Ok(())
}

#[test]
fn session_basic() -> Result<(), Box<dyn std::error::Error>> {
    match clear_session_file("test_basic".to_string()) {
        Err(why) => panic!("failed to remove session file : {}", why),
        Ok(()) => (),
    }

    let mut cmd = Command::cargo_bin("ht")?;
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

    "#});

    cmd = Command::cargo_bin("ht")?;
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

    "#});

    Ok(())
}
