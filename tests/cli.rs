#![cfg(feature = "integration-tests")]
#![allow(clippy::bool_assert_comparison)]
use std::{
    collections::HashSet,
    fs::File,
    fs::{create_dir_all, read_to_string, OpenOptions},
    io::{Seek, SeekFrom, Write},
    process::Command,
    time::Duration,
};

use assert_cmd::prelude::*;
use httpmock::{HttpMockRequest, Method::*, MockServer};
use indoc::{formatdoc, indoc};
use predicates::str::contains;
use serde_json::json;
use tempfile::{tempdir, tempfile};

fn random_string() -> String {
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
}

fn get_base_command() -> Command {
    let mut cmd = Command::cargo_bin("xh").expect("binary should be present");
    cmd.env("HOME", "");
    #[cfg(target_os = "windows")]
    cmd.env("XH_TEST_MODE_WIN_HOME_DIR", "");
    cmd
}

/// Sensible default command to test with. use [`get_base_command`] if this
/// setup doesn't apply.
fn get_command() -> Command {
    let mut cmd = get_base_command();
    cmd.env("XH_TEST_MODE", "1");
    cmd.env("XH_TEST_MODE_TERM", "1");
    cmd
}

/// Do not pretend the output goes to a terminal.
fn redirecting_command() -> Command {
    let mut cmd = get_base_command();
    cmd.env("XH_TEST_MODE", "1");
    cmd
}

/// Color output (with ANSI colors) by default.
fn color_command() -> Command {
    let mut cmd = get_command();
    cmd.env("XH_TEST_MODE_COLOR", "1");
    cmd
}

#[test]
fn basic_json_post() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .header("Content-Type", "application/json")
            .json_body(json!({"name": "ali"}));
        then.header("Content-Type", "application/json")
            .json_body(json!({"got": "name", "status": "ok"}));
    });

    get_command()
        .arg("--print=b")
        .arg("--pretty=format")
        .arg("post")
        .arg(server.base_url())
        .arg("name=ali")
        .assert()
        .stdout(indoc! {r#"
        {
            "got": "name",
            "status": "ok"
        }


        "#});
    mock.assert();
}

#[test]
fn basic_get() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET);
        then.body("foobar\n");
    });

    get_command()
        .arg("--print=b")
        .arg("get")
        .arg(server.base_url())
        .assert()
        .stdout("foobar\n\n");
    mock.assert();
}

#[test]
fn basic_head() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(HEAD);
    });

    get_command().arg("head").arg(server.base_url()).assert();
    mock.assert();
}

#[test]
fn basic_options() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(OPTIONS);
        then.header("Allow", "GET, HEAD, OPTIONS");
    });

    get_command()
        .arg("-h")
        .arg("options")
        .arg(server.base_url())
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"))
        .stdout(contains("Allow:"));
    mock.assert();
}

#[test]
fn multiline_value() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(POST).body("foo=bar%0Abaz");
    });

    get_command()
        .arg("--form")
        .arg("post")
        .arg(server.base_url())
        .arg("foo=bar\nbaz")
        .assert();
    mock.assert();
}

#[test]
fn header() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("X-Foo", "Bar");
    });
    get_command()
        .arg(server.base_url())
        .arg("x-foo:Bar")
        .assert();
    mock.assert();
}

#[test]
fn query_param() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.query_param("foo", "bar");
    });
    get_command()
        .arg(server.base_url())
        .arg("foo==bar")
        .assert();
    mock.assert();
}

#[test]
fn json_param() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.json_body(json!({"foo": [1, 2, 3]}));
    });
    get_command()
        .arg(server.base_url())
        .arg("foo:=[1,2,3]")
        .assert();
    mock.assert();
}

#[test]
fn verbose() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.header("Connection", "keep-alive")
            .header("Content-Type", "application/json")
            .header("Content-Length", "9")
            .header("User-Agent", "xh/0.0.0 (test mode)")
            .json_body(json!({"x": "y"}));
        then.body("a body")
            .header("date", "N/A")
            .header("X-Foo", "Bar");
    });
    get_command()
        .arg("--verbose")
        .arg(server.base_url())
        .arg("x=y")
        .assert()
        .stdout(indoc! {r#"
        POST / HTTP/1.1
        Accept: application/json, */*;q=0.5
        Accept-Encoding: gzip, deflate, br
        Connection: keep-alive
        Content-Length: 9
        Content-Type: application/json
        Host: http.mock
        User-Agent: xh/0.0.0 (test mode)

        {
            "x": "y"
        }



        HTTP/1.1 200 OK
        Content-Length: 6
        Date: N/A
        X-Foo: Bar

        a body
        "#});
    mock.assert();
}

#[test]
fn download() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.body("file contents\n");
    });

    let outfile = dir.path().join("outfile");
    get_command()
        .arg("--download")
        .arg("--output")
        .arg(&outfile)
        .arg(server.base_url())
        .assert();
    mock.assert();
    assert_eq!(read_to_string(&outfile).unwrap(), "file contents\n");
}

#[test]
fn accept_encoding_not_modifiable_in_download_mode() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.header("accept-encoding", "identity");
        then.body(r#"{"ids":[1,2,3]}"#);
    });

    let dir = tempdir().unwrap();
    get_command()
        .current_dir(&dir)
        .arg(server.base_url())
        .arg("--download")
        .arg("accept-encoding:gzip")
        .assert();
    mock.assert();
}

fn get_proxy_command(
    protocol_to_request: &str,
    protocol_to_proxy: &str,
    proxy_url: &str,
) -> Command {
    let mut cmd = get_command();
    cmd.arg("--pretty=format")
        .arg("--check-status")
        .arg(format!("--proxy={}:{}", protocol_to_proxy, proxy_url))
        .arg("GET")
        .arg(format!("{}://example.test/get", protocol_to_request));
    cmd
}

#[test]
fn proxy_http_proxy() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET).header("host", "example.test");
        then.status(200);
    });

    get_proxy_command("http", "http", &server.base_url())
        .assert()
        .success();

    mock.assert();
}

#[test]
fn proxy_https_proxy() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(CONNECT);
        then.status(502);
    });

    get_proxy_command("https", "https", &server.base_url())
        .assert()
        .stderr(contains("unsuccessful tunnel"))
        .failure();
    mock.assert();
}

#[test]
fn download_generated_filename() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    server.mock(|_when, then| {
        then.header("Content-Type", "application/json").body("file");
    });

    get_command()
        .arg("--download")
        .arg(server.url("/foo/bar/"))
        .current_dir(&dir)
        .assert();

    get_command()
        .arg("--download")
        .arg(server.url("/foo/bar/"))
        .current_dir(&dir)
        .assert();

    assert_eq!(read_to_string(dir.path().join("bar.json")).unwrap(), "file");
    assert_eq!(
        read_to_string(dir.path().join("bar.json-1")).unwrap(),
        "file"
    );
}

#[test]
fn download_supplied_filename() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Disposition", r#"attachment; filename="foo.bar""#)
            .body("file");
    });

    get_command()
        .arg("--download")
        .arg(server.base_url())
        .current_dir(&dir)
        .assert();
    mock.assert();
    assert_eq!(read_to_string(dir.path().join("foo.bar")).unwrap(), "file");
}

#[test]
fn download_supplied_unquoted_filename() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Disposition", r#"attachment; filename=foo bar baz"#)
            .body("file");
    });

    get_command()
        .arg("--download")
        .arg(server.base_url())
        .current_dir(&dir)
        .assert();
    mock.assert();
    assert_eq!(
        read_to_string(dir.path().join("foo bar baz")).unwrap(),
        "file"
    );
}

#[test]
fn decode() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9");
    });

    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout("é\n");
    mock.assert();
}

#[test]
fn proxy_all_proxy() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(CONNECT);
        then.status(502);
    });

    get_proxy_command("https", "all", &server.base_url())
        .assert()
        .stderr(contains("unsuccessful tunnel"))
        .failure();
    mock.assert();

    get_proxy_command("http", "all", &server.base_url())
        .assert()
        .failure();
    mock.assert();
}

#[test]
fn streaming_decode() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9");
    });

    get_command()
        .arg("--print=b")
        .arg("--stream")
        .arg(server.base_url())
        .assert()
        .stdout("é\n");
    mock.assert();
}

#[test]
fn only_decode_for_terminal() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9");
    });

    let output = redirecting_command()
        .arg(server.base_url())
        .assert()
        .get_output()
        .stdout
        .clone();
    assert_eq!(&output, b"\xe9"); // .stdout() doesn't support byte slices
    mock.assert();
}

#[test]
fn do_decode_if_formatted() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9");
    });

    redirecting_command()
        .arg("--pretty=all")
        .arg(server.base_url())
        .assert()
        .stdout("é");
    mock.assert();
}

#[test]
fn never_decode_if_binary() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        // this mimetype with a charset may actually be incoherent
        then.header("Content-Type", "application/octet-stream; charset=latin1")
            .body(b"\xe9");
    });

    let output = redirecting_command()
        .arg("--pretty=all")
        .arg(server.base_url())
        .assert()
        .get_output()
        .stdout
        .clone();
    assert_eq!(&output, b"\xe9");
    mock.assert();
}

#[test]
fn binary_detection() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.body(b"foo\0bar");
    });

    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        +-----------------------------------------+
        | NOTE: binary data not shown in terminal |
        +-----------------------------------------+

        "#});
    mock.assert();
}

#[test]
fn streaming_binary_detection() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.body(b"foo\0bar");
    });

    get_command()
        .arg("--print=b")
        .arg("--stream")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        +-----------------------------------------+
        | NOTE: binary data not shown in terminal |
        +-----------------------------------------+

        "#});
    mock.assert();
}

#[test]
fn request_binary_detection() {
    let mut binary_file = tempfile().unwrap();
    binary_file.write_all(b"foo\0bar").unwrap();
    binary_file.seek(SeekFrom::Start(0)).unwrap();
    redirecting_command()
        .arg("--print=B")
        .arg("--offline")
        .arg(":")
        .stdin(binary_file)
        .assert()
        .stdout(indoc! {r#"
        +-----------------------------------------+
        | NOTE: binary data not shown in terminal |
        +-----------------------------------------+


        "#});
}

#[test]
fn timeout() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.status(200).delay(Duration::from_secs_f32(0.5));
    });

    get_command()
        .arg("--timeout=0.1")
        .arg(server.base_url())
        .assert()
        .failure()
        .stderr(predicates::str::contains("operation timed out"));

    mock.assert();
}

#[test]
fn timeout_no_limit() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.status(200).delay(Duration::from_secs_f32(0.5));
    });

    get_command()
        .arg("--timeout=0")
        .arg(server.base_url())
        .assert()
        .success();

    mock.assert();
}

#[test]
fn timeout_invalid() {
    get_command()
        .arg("--timeout=-0.01")
        .arg("--offline")
        .arg(":")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Invalid seconds as connection timeout",
        ));
}

#[test]
fn last_supplied_proxy_wins() {
    let first_server = MockServer::start();
    let first_mock = first_server.mock(|when, then| {
        when.method(GET).header("host", "example.test");
        then.status(500);
    });
    let second_server = MockServer::start();
    let second_mock = second_server.mock(|when, then| {
        when.method(GET).header("host", "example.test");
        then.status(200);
    });

    let mut cmd = get_command();
    cmd.args(&[
        format!("--proxy=http:{}", first_server.base_url()).as_str(),
        format!("--proxy=http:{}", second_server.base_url()).as_str(),
        "GET",
        "http://example.test",
    ])
    .assert()
    .success();

    first_mock.assert_hits(0);
    second_mock.assert();
}

#[test]
fn proxy_multiple_valid_proxies() {
    let mut cmd = get_command();
    cmd.arg("--offline")
        .arg("--pretty=format")
        .arg("--proxy=http:https://127.0.0.1:8000")
        .arg("--proxy=https:socks5://127.0.0.1:8000")
        .arg("--proxy=all:http://127.0.0.1:8000")
        .arg("GET")
        .arg("http://httpbin.org/get");

    cmd.assert().success();
}

#[test]
fn check_status() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.status(404);
    });

    get_command()
        .arg("--check-status")
        .arg(server.base_url())
        .assert()
        .code(4)
        .stderr("");
    mock.assert();
}

#[test]
fn check_status_is_implied() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.status(404);
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .code(4)
        .stderr("");
    mock.assert();
}

#[test]
fn check_status_is_not_implied_in_compat_mode() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.status(404);
    });

    get_command()
        .env("XH_HTTPIE_COMPAT_MODE", "")
        .arg(server.base_url())
        .assert()
        .code(0);
    mock.assert();
}

#[test]
fn user_password_auth() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("Authorization", "Basic dXNlcjpwYXNz");
    });

    get_command()
        .arg("--auth=user:pass")
        .arg(server.base_url())
        .assert();
    mock.assert();
}

#[test]
fn netrc_env_user_password_auth() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("Authorization", "Basic dXNlcjpwYXNz");
    });

    let mut netrc = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        netrc,
        "machine {}\nlogin user\npassword pass",
        server.host()
    )
    .unwrap();

    get_command()
        .env("NETRC", netrc.path())
        .arg(server.base_url())
        .assert();
    mock.assert();
}

#[test]
fn netrc_file_user_password_auth() {
    for netrc_file in [".netrc", "_netrc"].iter() {
        let server = MockServer::start();
        let mock = server.mock(|when, _then| {
            when.header("Authorization", "Basic dXNlcjpwYXNz");
        });

        let homedir = tempfile::TempDir::new().unwrap();
        let netrc_path = homedir.path().join(netrc_file);
        let mut netrc = File::create(&netrc_path).unwrap();
        writeln!(
            netrc,
            "machine {}\nlogin user\npassword pass",
            server.host()
        )
        .unwrap();

        netrc.flush().unwrap();

        get_command()
            .env("HOME", homedir.path())
            .env("XH_TEST_MODE_WIN_HOME_DIR", homedir.path())
            .arg(server.base_url())
            .assert();

        mock.assert();

        drop(netrc);
        homedir.close().unwrap();
    }
}

#[test]
fn check_status_warning() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.status(501);
    });

    redirecting_command()
        .arg("--check-status")
        .arg(server.base_url())
        .assert()
        .code(5)
        .stderr("xh: warning: HTTP 501 Not Implemented\n");
    mock.assert();
}

#[test]
fn user_auth() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("Authorization", "Basic dXNlcjo=");
    });

    get_command()
        .arg("--auth=user:")
        .arg(server.base_url())
        .assert();
    mock.assert();
}

#[test]
fn bearer_auth() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("Authorization", "Bearer SomeToken");
    });

    get_command()
        .arg("--bearer=SomeToken")
        .arg(server.base_url())
        .assert();
    mock.assert();
}

// TODO: test implicit download filenames
// For this we have to pretend the output is a tty
// This intersects with both #41 and #59

#[test]
fn verify_default_yes() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("get")
        .arg("https://self-signed.badssl.com")
        .assert()
        .failure()
        .stdout(predicates::str::contains("GET / HTTP/1.1"))
        .stderr(predicates::str::contains("UnknownIssuer"));
}

#[test]
fn verify_explicit_yes() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("--verify=yes")
        .arg("get")
        .arg("https://self-signed.badssl.com")
        .assert()
        .failure()
        .stdout(predicates::str::contains("GET / HTTP/1.1"))
        .stderr(predicates::str::contains("UnknownIssuer"));
}

#[test]
fn verify_no() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("--verify=no")
        .arg("get")
        .arg("https://self-signed.badssl.com")
        .assert()
        .stdout(predicates::str::contains("GET / HTTP/1.1"))
        .stdout(predicates::str::contains("HTTP/1.1 200 OK"))
        .stderr(predicates::str::is_empty());
}

#[test]
fn verify_valid_file() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("--verify=tests/fixtures/certs/wildcard-self-signed.pem")
        .arg("get")
        .arg("https://self-signed.badssl.com")
        .assert()
        .stdout(predicates::str::contains("GET / HTTP/1.1"))
        .stdout(predicates::str::contains("HTTP/1.1 200 OK"))
        .stderr(predicates::str::is_empty());
}

// This test may fail if https://github.com/seanmonstar/reqwest/issues/1260 is fixed
// If that happens make sure to remove the warning, not just this test
#[cfg(feature = "native-tls")]
#[test]
fn verify_valid_file_native_tls() {
    get_command()
        .arg("--native-tls")
        .arg("--verify=tests/fixtures/certs/wildcard-self-signed.pem")
        .arg("https://self-signed.badssl.com")
        .assert()
        .stderr(predicates::str::contains(
            "Custom CA bundles with native-tls are broken",
        ));
}

#[test]
fn cert_without_key() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("get")
        .arg("https://client.badssl.com")
        .assert()
        .stdout(predicates::str::contains(
            "400 No required SSL certificate was sent",
        ))
        .stderr(predicates::str::is_empty());
}

#[test]
fn cert_with_key() {
    get_command()
        .arg("-v")
        .arg("--pretty=format")
        .arg("--cert=tests/fixtures/certs/client.badssl.com.crt")
        .arg("--cert-key=tests/fixtures/certs/client.badssl.com.key")
        .arg("get")
        .arg("https://client.badssl.com")
        .assert()
        .stdout(predicates::str::contains("HTTP/1.1 200 OK"))
        .stdout(predicates::str::contains("client-authenticated"))
        .stderr(predicates::str::is_empty());
}

#[cfg(feature = "native-tls")]
#[test]
fn cert_with_key_native_tls() {
    get_command()
        .arg("--native-tls")
        .arg("--cert=tests/fixtures/certs/client.badssl.com.crt")
        .arg("--cert-key=tests/fixtures/certs/client.badssl.com.key")
        .arg("https://client.badssl.com")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Client certificates are not supported for native-tls",
        ));
}

#[cfg(not(feature = "native-tls"))]
#[test]
fn native_tls_flag_disabled() {
    get_command()
        .arg("--native-tls")
        .arg(":")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "built without native-tls support",
        ));
}

#[cfg(not(feature = "native-tls"))]
#[test]
fn improved_https_ip_error_no_support() {
    get_command()
        .arg("https://1.1.1.1")
        .assert()
        .failure()
        .stderr(predicates::str::contains("rustls does not support"))
        .stderr(predicates::str::contains(
            "building with the `native-tls` feature",
        ));
}

#[cfg(feature = "native-tls")]
#[test]
fn native_tls_works() {
    get_command()
        .arg("--native-tls")
        .arg("https://example.org")
        .assert()
        .success();
}

#[cfg(feature = "native-tls")]
#[test]
fn improved_https_ip_error_with_support() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.permanent_redirect("https://1.1.1.1");
    });
    get_command()
        .arg("--follow")
        .arg(server.base_url())
        .assert()
        .failure()
        .stderr(predicates::str::contains("rustls does not support"))
        .stderr(predicates::str::contains("using the --native-tls flag"));
    mock.assert();
}

#[cfg(feature = "native-tls")]
#[test]
fn auto_nativetls() {
    get_command()
        .arg("--offline")
        .arg("https://1.1.1.1")
        .assert()
        .success()
        .stderr(predicates::str::contains("native-tls will be enabled"));
}

#[test]
fn good_tls_version() {
    get_command()
        .arg("--ssl=tls1.2")
        .arg("https://tls-v1-2.badssl.com:1012/")
        .assert()
        .success();
}

#[cfg(feature = "native-tls")]
#[test]
fn good_tls_version_nativetls() {
    get_command()
        .arg("--ssl=tls1.1")
        .arg("--native-tls")
        .arg("https://tls-v1-1.badssl.com:1011/")
        .assert()
        .success();
}

#[test]
fn bad_tls_version() {
    get_command()
        .arg("--ssl=tls1.3")
        .arg("https://tls-v1-2.badssl.com:1012/")
        .assert()
        .failure();
}

#[cfg(feature = "native-tls")]
#[test]
fn bad_tls_version_nativetls() {
    get_command()
        .arg("--ssl=tls1.1")
        .arg("--native-tls")
        .arg("https://tls-v1-2.badssl.com:1012/")
        .assert()
        .failure();
}

#[cfg(feature = "native-tls")]
#[test]
fn unsupported_tls_version_nativetls() {
    get_command()
        .arg("--ssl=tls1.3")
        .arg("--native-tls")
        .arg("https://example.org")
        .assert()
        .failure()
        .stderr(contains("invalid minimum TLS version"))
        .stderr(contains("running without the --native-tls"));
}

#[test]
fn unsupported_tls_version_rustls() {
    #[cfg(feature = "native-tls")]
    const MSG: &str = "native-tls will be enabled";
    #[cfg(not(feature = "native-tls"))]
    const MSG: &str = "Consider building with the `native-tls` feature enabled";

    get_command()
        .arg("--offline")
        .arg("--ssl=tls1.1")
        .arg(":")
        .assert()
        .stderr(contains("rustls does not support older TLS versions"))
        .stderr(contains(MSG));
}

#[test]
fn forced_json() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(GET)
            .header("content-type", "application/json")
            .header("accept", "application/json, */*;q=0.5");
    });
    get_command()
        .arg("--json")
        .arg(server.base_url())
        .assert()
        .success();
    mock.assert();
}

#[test]
fn forced_form() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(GET)
            .header("content-type", "application/x-www-form-urlencoded");
    });
    get_command()
        .arg("--form")
        .arg(server.base_url())
        .assert()
        .success();
    mock.assert();
}

#[test]
fn forced_multipart() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.method(POST).header_exists("content-type").body("");
    });
    get_command()
        .arg("--multipart")
        .arg(server.base_url())
        .assert()
        .success();
    mock.assert();
}

#[test]
fn formatted_json_output() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("content-type", "application/json")
            .body(r#"{"":0}"#);
    });
    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {
            "": 0
        }


        "#});
    mock.assert();
}

#[test]
fn inferred_json_output() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("content-type", "text/plain").body(r#"{"":0}"#);
    });
    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {
            "": 0
        }


        "#});
    mock.assert();
}

#[test]
fn inferred_json_javascript_output() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("content-type", "application/javascript")
            .body(r#"{"":0}"#);
    });
    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {
            "": 0
        }


        "#});
    mock.assert();
}

#[test]
fn inferred_nonjson_output() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        // Trailing comma makes it invalid JSON, though formatting would still work
        then.header("content-type", "text/plain").body(r#"{"":0,}"#);
    });
    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {"":0,}
        "#});
    mock.assert();
}

#[test]
fn noninferred_json_output() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        // Valid JSON, but not declared as text
        then.header("content-type", "application/octet-stream")
            .body(r#"{"":0}"#);
    });
    get_command()
        .arg("--print=b")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {"":0}
        "#});
    mock.assert();
}

#[test]
fn mixed_stdin_request_items() {
    let input_file = tempfile().unwrap();
    redirecting_command()
        .arg("--offline")
        .arg(":")
        .arg("x=3")
        .stdin(input_file)
        .assert()
        .failure()
        .stderr(contains(
            "Request body (from stdin) and request data (key=value) cannot be mixed",
        ));
}

#[test]
fn multipart_stdin() {
    let input_file = tempfile().unwrap();
    redirecting_command()
        .arg("--offline")
        .arg("--multipart")
        .arg(":")
        .stdin(input_file)
        .assert()
        .failure()
        .stderr(contains("Cannot build a multipart request body from stdin"));
}

#[test]
fn default_json_for_raw_body() {
    let server = MockServer::start();
    let mock = server.mock(|when, _then| {
        when.header("content-type", "application/json");
    });
    let input_file = tempfile().unwrap();
    redirecting_command()
        .arg(server.base_url())
        .stdin(input_file)
        .assert()
        .success();
    mock.assert();
}

#[test]
fn multipart_file_upload() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        // This test may be fragile, it's conceivable that the headers will become
        // lowercase in the future
        // (so if this breaks all of a sudden, check that first)
        when.body_contains("Hello world")
            .body_contains(concat!(
                "Content-Disposition: form-data; name=\"x\"; filename=\"input.txt\"\r\n",
                "\r\n",
                "Hello world\n"
            ))
            .body_contains(concat!(
                "Content-Disposition: form-data; name=\"y\"; filename=\"foobar.htm\"\r\n",
                "Content-Type: text/html\r\n",
                "\r\n",
                "Hello world\n",
            ));
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg("--form")
        .arg(server.base_url())
        .arg(format!("x@{}", filename.to_string_lossy()))
        .arg(format!(
            "y@{};type=text/html;filename=foobar.htm",
            filename.to_string_lossy()
        ))
        .assert()
        .success();

    mock.assert();
}

#[test]
fn body_from_file() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.header("content-type", "text/plain")
            .body("Hello world\n");
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("@{}", filename.to_string_lossy()))
        .assert()
        .success();

    mock.assert();
}

#[test]
fn body_from_file_with_explicit_mimetype() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.header("content-type", "image/png")
            .body("Hello world\n");
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("@{};type=image/png", filename.to_string_lossy()))
        .assert()
        .success();

    mock.assert();
}

#[test]
fn body_from_file_with_fallback_mimetype() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.header("content-type", "application/json")
            .body("Hello world\n");
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input");
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("@{}", filename.to_string_lossy()))
        .assert()
        .success();

    mock.assert();
}

#[test]
fn no_double_file_body() {
    get_command()
        .arg(":")
        .arg("@foo")
        .arg("@bar")
        .assert()
        .failure()
        .stderr(contains("Can't read request from multiple files"));
}

#[test]
fn print_body_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input");
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello world\n")
        .unwrap();

    get_command()
        .arg("--offline")
        .arg(":")
        .arg(format!("@{}", filename.to_string_lossy()))
        .assert()
        .success()
        .stdout(contains("Hello world"));
}

#[test]
fn colored_headers() {
    color_command()
        .arg("--offline")
        .arg(":")
        .assert()
        .success()
        // Color
        .stdout(contains("\x1b[4m"))
        // Reset
        .stdout(contains("\x1b[0m"));
}

#[test]
fn colored_body() {
    color_command()
        .arg("--offline")
        .arg(":")
        .arg("x:=3")
        .assert()
        .success()
        .stdout(contains("\x1b[34m3\x1b[0m"));
}

#[test]
fn force_color_pipe() {
    redirecting_command()
        .arg("--ignore-stdin")
        .arg("--offline")
        .arg("--pretty=colors")
        .arg(":")
        .arg("x:=3")
        .assert()
        .success()
        .stdout(contains("\x1b[34m3\x1b[0m"));
}

#[test]
fn request_json_keys_order_is_preserved() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.body(r#"{"name":"ali","age":24}"#);
    });

    get_command()
        .arg("get")
        .arg(server.base_url())
        .arg("name=ali")
        .arg("age:=24")
        .assert();
    mock.assert();
}

#[test]
fn data_field_from_file() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.body(r#"{"ids":"[1,2,3]"}"#);
    });

    let mut text_file = tempfile::NamedTempFile::new().unwrap();
    write!(text_file, "[1,2,3]").unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("ids=@{}", text_file.path().to_string_lossy()))
        .assert();
    mock.assert();
}

#[test]
fn data_field_from_file_in_form_mode() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.body(r#"message=hello+world"#);
    });

    let mut text_file = tempfile::NamedTempFile::new().unwrap();
    write!(text_file, "hello world").unwrap();

    get_command()
        .arg(server.base_url())
        .arg("--form")
        .arg(format!("message=@{}", text_file.path().to_string_lossy()))
        .assert();
    mock.assert();
}

#[test]
fn json_field_from_file() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.body(r#"{"ids":[1,2,3]}"#);
    });

    let mut json_file = tempfile::NamedTempFile::new().unwrap();
    writeln!(json_file, "[1,2,3]").unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("ids:=@{}", json_file.path().to_string_lossy()))
        .assert();
    mock.assert();
}

#[test]
fn can_unset_default_headers() {
    get_command()
        .arg(":")
        .arg("user-agent:")
        .arg("--offline")
        .assert()
        .stdout(indoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock

        "#});
}

#[test]
fn can_unset_headers() {
    get_command()
        .arg(":")
        .arg("hello:world")
        .arg("goodby:world")
        .arg("goodby:")
        .arg("--offline")
        .assert()
        .stdout(indoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Hello: world
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

        "#});
}

#[test]
fn can_set_unset_header() {
    get_command()
        .arg(":")
        .arg("hello:")
        .arg("hello:world")
        .arg("--offline")
        .assert()
        .stdout(indoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Hello: world
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

        "#});
}

// httpmock's matches function doesn't accept closures
// see https://github.com/alexliesenfeld/httpmock/issues/44#issuecomment-840797442
macro_rules! cookie_exists {
    ($when:ident, $expected_value:expr) => {{
        $when.matches(|req: &HttpMockRequest| {
            req.headers
                .as_ref()
                .unwrap()
                .iter()
                .any(|(key, actual_value)| {
                    key == "cookie" && {
                        let expected = $expected_value.split("; ").collect::<HashSet<_>>();
                        let actual = actual_value.split("; ").collect::<HashSet<_>>();
                        actual == expected
                    }
                })
        });
    }};
}

#[test]
fn named_sessions() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.header("set-cookie", "cook1=one; Path=/");
    });

    let config_dir = tempdir().unwrap();
    let random_name = random_string();

    get_command()
        .env("XH_CONFIG_DIR", config_dir.path())
        .arg(server.base_url())
        .arg(format!("--session={}", random_name))
        .arg("--bearer=hello")
        .arg("cookie:lang=en")
        .assert()
        .success();

    mock.assert();

    let path_to_session = config_dir.path().join::<std::path::PathBuf>(
        [
            "sessions",
            &format!("127.0.0.1_{}", server.port()),
            &format!("{}.json", random_name),
        ]
        .iter()
        .collect(),
    );

    let session_content = read_to_string(path_to_session).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": {
                "about": "xh session file",
                "xh": "0.0.0"
            },
            "auth": { "type": "bearer", "raw_auth": "hello" },
            "cookies": {
                "cook1": { "value": "one", "path": "/" },
                "lang": { "value": "en" }
            },
            "headers": {}
        })
    );
}

#[test]
fn anonymous_sessions() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.header("set-cookie", "cook1=one");
    });

    let mut path_to_session = std::env::temp_dir();
    let file_name = random_string();
    path_to_session.push(file_name);

    get_command()
        .arg(server.base_url())
        .arg(format!("--session={}", path_to_session.to_string_lossy()))
        .arg("--auth=me:pass")
        .arg("hello:world")
        .assert()
        .success();

    mock.assert();

    let session_content = read_to_string(path_to_session).unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": {
                "about": "xh session file",
                "xh": "0.0.0"
            },
            "auth": { "type": "basic", "raw_auth": "me:pass" },
            "cookies": { "cook1": { "value": "one" } },
            "headers": { "hello": "world" }
        })
    );
}

#[test]
fn anonymous_read_only_session() {
    let server = MockServer::start();
    server.mock(|_, then| {
        then.header("set-cookie", "lang=en");
    });

    let session_file = tempfile::NamedTempFile::new().unwrap();
    let old_session_content = serde_json::json!({
        "__meta__": { "about": "xh session file", "xh": "0.0.0" },
        "auth": { "type": null, "raw_auth": null },
        "cookies": { "cookie1": { "value": "one" } },
        "headers": { "hello": "world" }
    });

    std::fs::write(&session_file, old_session_content.to_string()).unwrap();

    get_command()
        .arg(server.base_url())
        .arg("goodbye:world")
        .arg(format!(
            "--session-read-only={}",
            session_file.path().to_string_lossy()
        ))
        .assert()
        .success();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read_to_string(session_file.path()).unwrap())
            .unwrap(),
        old_session_content
    );
}

#[test]
fn session_files_are_created_in_read_only_mode() {
    let server = MockServer::start();
    server.mock(|_, then| {
        then.header("set-cookie", "lang=ar");
    });

    let mut path_to_session = std::env::temp_dir();
    let file_name = random_string();
    path_to_session.push(file_name);
    assert_eq!(path_to_session.exists(), false);

    get_command()
        .arg(server.base_url())
        .arg("hello:world")
        .arg(format!(
            "--session-read-only={}",
            path_to_session.to_string_lossy()
        ))
        .assert()
        .success();

    let session_content = read_to_string(path_to_session).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": {
                "about": "xh session file",
                "xh": "0.0.0"
            },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {
                "lang": { "value": "ar" }
            },
            "headers": {
                "hello": "world"
            }
        })
    );
}

#[test]
fn named_read_only_session() {
    let server = MockServer::start();
    server.mock(|_, then| {
        then.header("set-cookie", "lang=en");
    });

    let config_dir = tempdir().unwrap();
    let random_name = random_string();
    let path_to_session = config_dir.path().join::<std::path::PathBuf>(
        [
            "xh",
            "sessions",
            &format!("127.0.0.1_{}", server.port()),
            &format!("{}.json", random_name),
        ]
        .iter()
        .collect(),
    );
    let old_session_content = serde_json::json!({
        "__meta__": { "about": "xh session file", "xh": "0.0.0" },
        "auth": { "type": null, "raw_auth": null },
        "cookies": {
            "cookie1": { "value": "one" }
        },
        "headers": {
            "hello": "world"
        }
    });
    create_dir_all(path_to_session.parent().unwrap()).unwrap();
    File::create(&path_to_session).unwrap();
    std::fs::write(&path_to_session, old_session_content.to_string()).unwrap();

    get_command()
        .env("XH_CONFIG_DIR", config_dir.path())
        .arg(server.base_url())
        .arg("goodbye:world")
        .arg(format!("--session-read-only={}", random_name))
        .assert()
        .success();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read_to_string(path_to_session).unwrap())
            .unwrap(),
        old_session_content
    );
}

#[test]
fn expired_cookies_are_removed_from_session() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let future_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 1000;
    let past_timestamp = 1114425967; // 2005-04-25

    let session_file = tempfile::NamedTempFile::new().unwrap();

    std::fs::write(
        &session_file,
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {
                "expired_cookie": {
                    "value": "random_string",
                    "expires": past_timestamp
                },
                "unexpired_cookie": {
                    "value": "random_string",
                    "expires": future_timestamp
                },
                "with_out_expiry": {
                    "value": "random_string",
                }
            },
            "headers": {}
        })
        .to_string(),
    )
    .unwrap();

    get_command()
        .arg(":")
        .arg(format!(
            "--session={}",
            session_file.path().to_string_lossy()
        ))
        .arg("--offline")
        .assert()
        .success();

    let session_content = read_to_string(session_file.path()).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {
                "unexpired_cookie": {
                    "value": "random_string",
                    "expires": future_timestamp
                },
                "with_out_expiry": {
                    "value": "random_string",
                }
            },
            "headers": {}
        })
    );
}

#[test]
fn cookies_override_each_other_in_the_correct_order() {
    // Cookies storage priority is: Server response > Command line request > Session file
    // See https://httpie.io/docs#cookie-storage-behaviour
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        cookie_exists!(when, "lang=fr; cook1=two; cook2=two");
        then.header("set-cookie", "lang=en")
            .header("set-cookie", "cook1=one");
    });

    let session_file = tempfile::NamedTempFile::new().unwrap();

    std::fs::write(
        &session_file,
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {
                "lang": { "value": "fr" },
                "cook2": { "value": "three" }
            },
            "headers": {}
        })
        .to_string(),
    )
    .unwrap();

    get_command()
        .arg(server.base_url())
        .arg("cookie:cook1=two;cook2=two")
        .arg(format!(
            "--session={}",
            session_file.path().to_string_lossy()
        ))
        .arg("--no-check-status")
        .assert()
        .success();

    mock.assert();

    let session_content = read_to_string(session_file.path()).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {
                "lang": { "value": "en" },
                "cook1": { "value": "one" },
                "cook2": { "value": "two" }
            },
            "headers": {}
        })
    );
}

#[test]
fn basic_auth_from_session_is_used() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.header("authorization", "Basic dXNlcjpwYXNz");
    });

    let session_file = tempfile::NamedTempFile::new().unwrap();

    std::fs::write(
        &session_file,
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": "basic", "raw_auth": "user:pass" },
            "cookies": {},
            "headers": {}
        })
        .to_string(),
    )
    .unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!(
            "--session={}",
            session_file.path().to_string_lossy()
        ))
        .arg("--no-check-status")
        .assert()
        .success();

    mock.assert();
}

#[test]
fn bearer_auth_from_session_is_used() {
    let server = MockServer::start();
    let mock = server.mock(|when, _| {
        when.header("authorization", "Bearer secret-token");
    });

    let session_file = tempfile::NamedTempFile::new().unwrap();

    std::fs::write(
        &session_file,
        serde_json::json!({
            "__meta__": { "about": "xh session file", "xh": "0.0.0" },
            "auth": { "type": "bearer", "raw_auth": "secret-token" },
            "cookies": {},
            "headers": {}
        })
        .to_string(),
    )
    .unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!(
            "--session={}",
            session_file.path().to_string_lossy()
        ))
        .arg("--no-check-status")
        .assert()
        .success();

    mock.assert();
}

#[test]
fn print_intermediate_requests_and_responses() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    server1.mock(|_, then| {
        then.header("location", &server2.base_url())
            .status(302)
            .header("date", "N/A")
            .body("redirecting...");
    });
    server2.mock(|_, then| {
        then.header("date", "N/A").body("final destination");
    });

    get_command()
        .arg(server1.base_url())
        .arg("--follow")
        .arg("--verbose")
        .arg("--all")
        .assert()
        .stdout(formatdoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: {url}

            redirecting...

            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#, url = server2.base_url() });
}

#[test]
fn history_print() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    server1.mock(|_, then| {
        then.header("location", &server2.base_url())
            .status(302)
            .header("date", "N/A")
            .body("redirecting...");
    });
    server2.mock(|_, then| {
        then.header("date", "N/A").body("final destination");
    });

    get_command()
        .arg(server1.base_url())
        .arg("--follow")
        .arg("--print=HhBb")
        .arg("--history-print=Hh")
        .arg("--all")
        .assert()
        .stdout(formatdoc! {r#"
            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: {url}

            GET / HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#, url = server2.base_url() });
}

#[test]
fn max_redirects_is_enforced() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    server1.mock(|_, then| {
        then.header("location", &server2.base_url())
            .status(302)
            .body("redirecting...");
    });
    server2.mock(|_, then| {
        then.header("location", &server2.base_url()) // redirect to the same server
            .status(302)
            .body("redirecting...");
    });

    get_command()
        .arg(server1.base_url())
        .arg("--follow")
        .arg("--max-redirects=5")
        .assert()
        .stderr(contains("Too many redirects (--max-redirects=5)"))
        .failure();
}

#[test]
fn method_is_changed_when_following_302_redirect() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    let mock1 = server1.mock(|when, then| {
        when.method(POST)
            .header_exists("Content-Length")
            .body(r#"{"name":"ali"}"#);
        then.header("location", &server2.base_url())
            .status(302)
            .body("redirecting...");
    });
    let mock2 = server2.mock(|when, then| {
        when.method(GET).matches(|req: &HttpMockRequest| {
            !req.headers
                .as_ref()
                .unwrap()
                .iter()
                .any(|(key, _)| key == "Content-Length")
        });
        then.body("final destination");
    });

    get_command()
        .arg("post")
        .arg(server1.base_url())
        .arg("--follow")
        .arg("name=ali")
        .assert()
        .success();

    mock1.assert();
    mock2.assert();
}

#[test]
fn method_is_not_changed_when_following_307_redirect() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    let mock1 = server1.mock(|when, then| {
        when.method(POST).body(r#"{"name":"ali"}"#);
        then.header("location", &server2.base_url())
            .status(307)
            .body("redirecting...");
    });
    let mock2 = server2.mock(|when, then| {
        when.method(POST).body(r#"{"name":"ali"}"#);
        then.body("final destination");
    });

    get_command()
        .arg("post")
        .arg(server1.base_url())
        .arg("--follow")
        .arg("name=ali")
        .assert()
        .success();

    mock1.assert();
    mock2.assert();
}

#[test]
fn sensitive_headers_are_removed_after_cross_domain_redirect() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    let mock1 = server1.mock(|when, then| {
        when.header_exists("Authorization").header_exists("hello");
        then.header("Location", &server2.base_url())
            .status(302)
            .body("redirecting...");
    });
    let mock2 = server2.mock(|when, then| {
        when.header_exists("Hello")
            .matches(|req: &HttpMockRequest| {
                !req.headers
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|(key, _)| key == "Authorization")
            });
        then.header("Date", "N/A").body("final destination");
    });

    get_command()
        .arg(server1.base_url())
        .arg("--follow")
        .arg("--auth=user:pass")
        .arg("hello:world")
        .assert()
        .success();

    mock1.assert();
    mock2.assert();
}

#[test]
fn request_body_is_buffered_for_307_redirect() {
    let server1 = MockServer::start();
    let server2 = MockServer::start();
    server1.mock(|_, then| {
        then.header("location", &server2.base_url())
            .status(307)
            .body("redirecting...");
    });
    let mock2 = server2.mock(|when, then| {
        when.body("hello world\n");
        then.body("final destination");
    });

    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "hello world").unwrap();

    get_command()
        .arg(server1.base_url())
        .arg("--follow")
        .arg(format!("@{}", file.path().to_string_lossy()))
        .assert()
        .success();

    mock2.assert();
}

#[test]
fn read_args_from_config() {
    let config_dir = tempdir().unwrap();
    File::create(config_dir.path().join("config.json")).unwrap();
    std::fs::write(
        config_dir.path().join("config.json"),
        serde_json::json!({"default_options": ["--form", "--print=hbHB"]}).to_string(),
    )
    .unwrap();

    get_command()
        .env("XH_CONFIG_DIR", config_dir.path())
        .arg(":")
        .arg("--offline")
        .arg("--print=B") // this should overwrite the value from config.json
        .arg("sort=asc")
        .arg("limit=100")
        .assert()
        .stdout("sort=asc&limit=100\n\n")
        .success();
}

#[test]
fn warns_if_config_is_invalid() {
    let config_dir = tempdir().unwrap();
    File::create(config_dir.path().join("config.json")).unwrap();
    std::fs::write(
        config_dir.path().join("config.json"),
        serde_json::json!({"default_options": "--form"}).to_string(),
    )
    .unwrap();

    get_command()
        .env("XH_CONFIG_DIR", config_dir.path())
        .arg(":")
        .arg("--offline")
        .assert()
        .stderr(contains("Unable to parse config file"))
        .success();
}

#[test]
fn http1_0() {
    get_command()
        .arg("--print=hH")
        .arg("--http-version=1.0")
        .arg("https://www.google.com")
        .assert()
        .success()
        .stdout(predicates::str::contains("GET / HTTP/1.0"))
        // Some servers i.e nginx respond with HTTP/1.1 to HTTP/1.0 requests, see https://serverfault.com/questions/442960/nginx-ignoring-clients-http-1-0-request-and-respond-by-http-1-1
        // Fortunately, https://www.google.com is not one of those.
        .stdout(predicates::str::contains("HTTP/1.0 200 OK"));
}

#[test]
fn http1_1() {
    get_command()
        .arg("--print=hH")
        .arg("--http-version=1.1")
        .arg("https://www.google.com")
        .assert()
        .success()
        .stdout(predicates::str::contains("GET / HTTP/1.1"))
        .stdout(predicates::str::contains("HTTP/1.1 200 OK"));
}

#[test]
fn http2() {
    get_command()
        .arg("--print=hH")
        .arg("--http-version=2")
        .arg("https://www.google.com")
        .assert()
        .success()
        .stdout(predicates::str::contains("GET / HTTP/2.0"))
        .stdout(predicates::str::contains("HTTP/2.0 200 OK"));
}

#[test]
fn override_response_charset() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/plain; charset=utf-8")
            .body(b"\xe9");
    });

    get_command()
        .arg("--print=b")
        .arg("--response-charset=latin1")
        .arg(server.base_url())
        .assert()
        .stdout("é\n");
    mock.assert();
}

#[test]
fn override_response_mime() {
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "text/html; charset=utf-8")
            .body("{\"status\": \"ok\"}");
    });

    get_command()
        .arg("--print=b")
        .arg("--response-mime=application/json")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
        {
            "status": "ok"
        }


        "#});
    mock.assert();
}
