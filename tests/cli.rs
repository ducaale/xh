#![cfg(feature = "integration-tests")]
use std::{
    fs::read_to_string,
    fs::File,
    io::{Seek, SeekFrom, Write},
    process::Command,
};

use assert_cmd::prelude::*;
use httpmock::{Method::*, MockServer};
use indoc::indoc;
use predicate::str::contains;
use predicates::prelude::*;
use serde_json::json;
use tempfile::{tempdir, tempfile};

pub fn random_string() -> String {
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
        .stdout(contains("allow:"));
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
        accept: application/json, */*;q=0.5
        accept-encoding: gzip, br
        connection: keep-alive
        content-length: 9
        content-type: application/json
        host: http.mock
        user-agent: xh/0.0.0 (test mode)

        {
            "x": "y"
        }



        HTTP/1.1 200 OK
        content-length: 6
        date: N/A
        x-foo: Bar

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
        .stderr(predicate::str::contains("unsuccessful tunnel"))
        .failure();
    mock.assert();
}

#[test]
fn download_generated_filename() {
    let dir = tempdir().unwrap();
    let server = MockServer::start();
    let mock = server.mock(|_when, then| {
        then.header("Content-Type", "application/json").body("file");
    });

    get_command()
        .arg("--download")
        .arg(server.url("/foo/bar/"))
        .current_dir(&dir)
        .assert();
    mock.assert();
    assert_eq!(read_to_string(dir.path().join("bar.json")).unwrap(), "file");
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
        .stderr(predicate::str::contains("unsuccessful tunnel"))
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
        .stderr("\nxh: warning: HTTP 501 Not Implemented\n\n");
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
        .stderr(predicate::str::contains(
            "Request body (from stdin) and Request data (key=value) cannot be mixed",
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
        .stderr(predicate::str::contains(
            "Cannot build a multipart request body from stdin",
        ));
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
fn colored_headers() {
    color_command()
        .arg("--offline")
        .arg(":")
        .assert()
        .success()
        // Color
        .stdout(predicate::str::contains("\x1b[4m"))
        // Reset
        .stdout(predicate::str::contains("\x1b[0m"));
}

#[test]
fn colored_body() {
    color_command()
        .arg("--offline")
        .arg(":")
        .arg("x:=3")
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b[34m3\x1b[0m"));
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
        .stdout(predicate::str::contains("\x1b[34m3\x1b[0m"));
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
            accept: */*
            accept-encoding: gzip, br
            connection: keep-alive
            host: http.mock

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
            accept: */*
            accept-encoding: gzip, br
            connection: keep-alive
            hello: world
            host: http.mock
            user-agent: xh/0.0.0 (test mode)

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
            accept: */*
            accept-encoding: gzip, br
            connection: keep-alive
            hello: world
            host: http.mock
            user-agent: xh/0.0.0 (test mode)

        "#});
}

#[test]
fn named_sessions() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.header("set-cookie", "cook1=one; Path=/");
    });

    let random_name = random_string();

    get_command()
        .arg(server.base_url())
        .arg(format!("--session={}", random_name))
        .arg("cookie:lang=en")
        .assert()
        .success();

    mock.assert();

    let path_to_session = dirs::config_dir().unwrap().join::<std::path::PathBuf>(
        [
            "xh-test",
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
            "cookies": {
                "cook1": {
                    "name": "cook1",
                    "value": "one"
                },
                "lang": {
                    "name": "lang",
                    "value": "en"
                }
            },
            "headers": {}
        })
    );
}

#[test]
fn anonymous_sessions() {
    let server = MockServer::start();
    let mock = server.mock(|_, then| {
        then.header("set-cookie", "cook1=one; Path=/");
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
            "cookies": {
                "cook1": {
                    "name": "cook1",
                    "value": "one"
                }
            },
            "headers": {
                "hello": "world",
                "authorization": "Basic bWU6cGFzcw=="
            }
        })
    );
}
