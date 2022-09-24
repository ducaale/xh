#![allow(clippy::bool_assert_comparison)]

mod server;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::Write;
use std::iter::FromIterator;
use std::net::IpAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use assert_cmd::cmd::Command;
use indoc::indoc;
use predicates::function::function;
use predicates::str::contains;
use tempfile::{tempdir, NamedTempFile, TempDir};

pub trait RequestExt {
    fn query_params(&self) -> HashMap<String, String>;
    fn body(self) -> Pin<Box<dyn Future<Output = Vec<u8>> + Send>>;
    fn body_as_string(self) -> Pin<Box<dyn Future<Output = String> + Send>>;
}

impl<T> RequestExt for hyper::Request<T>
where
    T: hyper::body::HttpBody + Send + 'static,
    T::Data: Send,
    T::Error: std::fmt::Debug,
{
    fn query_params(&self) -> HashMap<String, String> {
        form_urlencoded::parse(self.uri().query().unwrap().as_bytes())
            .into_owned()
            .collect::<HashMap<String, String>>()
    }

    fn body(self) -> Pin<Box<dyn Future<Output = Vec<u8>> + Send>> {
        let fut = async {
            hyper::body::to_bytes(self)
                .await
                .unwrap()
                .as_ref()
                .to_owned()
        };
        Box::pin(fut)
    }

    fn body_as_string(self) -> Pin<Box<dyn Future<Output = String> + Send>> {
        let fut = async { String::from_utf8(self.body().await).unwrap() };
        Box::pin(fut)
    }
}

fn random_string() -> String {
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
}

/// Cargo-cross for ARM runs tests using qemu.
///
/// It sets an environment variable like this:
/// CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUNNER=qemu-arm
fn find_runner() -> Option<String> {
    for (key, value) in std::env::vars() {
        if key.starts_with("CARGO_TARGET_") && key.ends_with("_RUNNER") && !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn get_base_command() -> Command {
    let mut cmd;
    let path = assert_cmd::cargo::cargo_bin("xh");
    if let Some(runner) = find_runner() {
        let mut runner = runner.split_whitespace();
        cmd = Command::new(runner.next().unwrap());
        for arg in runner {
            cmd.arg(arg);
        }
        cmd.arg(path);
    } else {
        cmd = Command::new(path);
    }
    cmd.env("HOME", "");
    cmd.env("NETRC", "");
    cmd.env("XH_CONFIG_DIR", "");
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

const BINARY_SUPPRESSOR: &str = concat!(
    "+-----------------------------------------+\n",
    "| NOTE: binary data not shown in terminal |\n",
    "+-----------------------------------------+\n",
    "\n"
);

#[test]
fn basic_json_post() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.headers()["Content-Type"], "application/json");
        assert_eq!(req.body_as_string().await, "{\"name\":\"ali\"}");

        hyper::Response::builder()
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(r#"{"got":"name","status":"ok"}"#.into())
            .unwrap()
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
}

#[test]
fn basic_get() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "GET");
        hyper::Response::builder().body("foobar\n".into()).unwrap()
    });
    get_command()
        .args(&["--print=b", "get", &server.base_url()])
        .assert()
        .stdout("foobar\n\n");
}

#[test]
fn basic_head() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "HEAD");
        hyper::Response::default()
    });
    get_command()
        .args(&["head", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn basic_options() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "OPTIONS");
        hyper::Response::builder()
            .header("Allow", "GET, HEAD, OPTIONS")
            .body("".into())
            .unwrap()
    });
    get_command()
        .args(&["-h", "options", &server.base_url()])
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"))
        .stdout(contains("Allow:"));
}

#[test]
fn multiline_value() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.body_as_string().await, "foo=bar%0Abaz");
        hyper::Response::default()
    });

    get_command()
        .args(&["--form", "post", &server.base_url(), "foo=bar\nbaz"])
        .assert()
        .success();
}

#[test]
fn nested_json() {
    let server = server::http(|req| async move {
        assert_eq!(
            req.body_as_string().await,
            r#"{"shallow":"value","object":{"key":"value"},"array":[1,2,3],"wow":{"such":{"deep":[null,null,null,{"much":{"power":{"!":"Amaze"}}}]}}}"#
        );
        hyper::Response::default()
    });

    get_command()
        .args(&["post", &server.base_url()])
        .arg("shallow=value")
        .arg("object[key]=value")
        .arg("array[]:=1")
        .arg("array[1]:=2")
        .arg("array[2]:=3")
        .arg("wow[such][deep][3][much][power][!]=Amaze")
        .assert()
        .success();
}

#[test]
fn json_path_with_escaped_characters() {
    get_command()
        .arg("--print=B")
        .arg("--offline")
        .arg(":")
        .arg(r"f\=\:\;oo\[\\[\@]=b\:\:\:ar")
        .assert()
        .stdout(indoc! {r#"
            {
                "f=:;oo[\\": {
                    "@": "b:::ar"
                }
            }



        "#});
}

#[test]
fn nested_json_type_error() {
    get_command()
        .arg("--print=B")
        .arg("--offline")
        .arg(":")
        .arg("x[x][2]=5")
        .arg("x[x][x]=2")
        .assert()
        .failure()
        .stderr(indoc! {r#"
            xh: error: Can't perform 'key' based access on 'x[x]' which has a type of 'array' but this operation requires a type of 'object'.

              x[x][x]
                  ^^^
        "#});

    get_command()
        .arg("--print=B")
        .arg("--offline")
        .arg(":")
        .arg("foo[x]=5")
        .arg("[][x]=2")
        .assert()
        .failure()
        .stderr(indoc! {r#"
            xh: error: Can't perform 'append' based access on '' which has a type of 'object' but this operation requires a type of 'array'.

              [][x]
              ^^
        "#});
}

#[test]
fn json_path_special_chars_not_escaped_in_form() {
    get_command()
        .arg("--print=B")
        .arg("--offline")
        .arg("--form")
        .arg(":")
        .arg(r"\]=a")
        .assert()
        .stdout(indoc! {r#"
            %5C%5D=a

        "#});
}

#[test]
fn header() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["X-Foo"], "Bar");
        hyper::Response::default()
    });
    get_command()
        .args(&[&server.base_url(), "x-foo:Bar"])
        .assert()
        .success();
}

#[test]
fn multiple_headers_with_same_key() {
    let server = server::http(|req| async move {
        let mut hello_header = req.headers().get_all("hello").iter();
        assert_eq!(hello_header.next().unwrap(), &"world");
        assert_eq!(hello_header.next().unwrap(), &"people");
        hyper::Response::default()
    });
    get_command()
        .args(&[&server.base_url(), "hello:world", "hello:people"])
        .assert()
        .success();
}

#[test]
fn query_param() {
    let server = server::http(|req| async move {
        assert_eq!(req.query_params()["foo"], "bar");
        hyper::Response::default()
    });
    get_command()
        .args(&[&server.base_url(), "foo==bar"])
        .assert()
        .success();
}

#[test]
fn json_param() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, "{\"foo\":[1,2,3]}");
        hyper::Response::default()
    });
    get_command()
        .args(&[&server.base_url(), "foo:=[1,2,3]"])
        .assert()
        .success();
}

#[test]
fn verbose() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Connection"], "keep-alive");
        assert_eq!(req.headers()["Content-Type"], "application/json");
        assert_eq!(req.headers()["Content-Length"], "9");
        assert_eq!(req.headers()["User-Agent"], "xh/0.0.0 (test mode)");
        assert_eq!(req.body_as_string().await, "{\"x\":\"y\"}");
        hyper::Response::builder()
            .header("X-Foo", "Bar")
            .header("Date", "N/A")
            .body("a body".into())
            .unwrap()
    });
    get_command()
        .args(&["--verbose", &server.base_url(), "x=y"])
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
}

#[test]
fn download() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .body("file contents\n".into())
            .unwrap()
    });

    let outfile = dir.path().join("outfile");
    get_command()
        .arg("--download")
        .arg("--output")
        .arg(&outfile)
        .arg(server.base_url())
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&outfile).unwrap(), "file contents\n");
}

#[test]
fn accept_encoding_not_modifiable_in_download_mode() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["accept-encoding"], "identity");
        hyper::Response::builder()
            .body(r#"{"ids":[1,2,3]}"#.into())
            .unwrap()
    });

    let dir = tempdir().unwrap();
    get_command()
        .current_dir(&dir)
        .args(&[&server.base_url(), "--download", "accept-encoding:gzip"])
        .assert()
        .success();
}

#[test]
fn download_generated_filename() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/json")
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(&["--download", &server.url("/foo/bar/")])
        .current_dir(&dir)
        .assert()
        .success();

    get_command()
        .args(&["--download", &server.url("/foo/bar/")])
        .current_dir(&dir)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(dir.path().join("bar.json")).unwrap(),
        "file"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("bar.json-1")).unwrap(),
        "file"
    );
}

#[test]
fn download_supplied_filename() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Disposition", r#"attachment; filename="foo.bar""#)
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(&["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("foo.bar")).unwrap(),
        "file"
    );
}

#[test]
fn download_supplied_unquoted_filename() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Disposition", r#"attachment; filename=foo bar baz"#)
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(&["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("foo bar baz")).unwrap(),
        "file"
    );
}

// TODO: test implicit download filenames
// For this we have to pretend the output is a tty
// This intersects with both #41 and #59

#[test]
fn decode() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });

    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout("é\n");
}

#[test]
fn streaming_decode() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });

    get_command()
        .args(&["--print=b", "--stream", &server.base_url()])
        .assert()
        .stdout("é\n");
}

#[test]
fn only_decode_for_terminal() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });

    let output = redirecting_command()
        .arg(server.base_url())
        .assert()
        .get_output()
        .stdout
        .clone();
    assert_eq!(&output, b"\xe9"); // .stdout() doesn't support byte slices
}

#[test]
fn do_decode_if_formatted() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/plain; charset=latin1")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });
    redirecting_command()
        .args(&["--pretty=all", &server.base_url()])
        .assert()
        .stdout("é");
}

#[test]
fn never_decode_if_binary() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            // this mimetype with a charset may actually be incoherent
            .header("Content-Type", "application/octet-stream; charset=latin1")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });

    let output = redirecting_command()
        .args(&["--pretty=all", &server.base_url()])
        .assert()
        .get_output()
        .stdout
        .clone();
    assert_eq!(&output, b"\xe9");
}

#[test]
fn binary_detection() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .body(b"foo\0bar".as_ref().into())
            .unwrap()
    });

    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(BINARY_SUPPRESSOR);
}

#[test]
fn streaming_binary_detection() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .body(b"foo\0bar".as_ref().into())
            .unwrap()
    });

    get_command()
        .args(&["--print=b", "--stream", &server.base_url()])
        .assert()
        .stdout(BINARY_SUPPRESSOR);
}

#[test]
fn request_binary_detection() {
    redirecting_command()
        .args(&["--print=B", "--offline", ":"])
        .write_stdin(b"foo\0bar".as_ref())
        .assert()
        .stdout(indoc! {r#"
            +-----------------------------------------+
            | NOTE: binary data not shown in terminal |
            +-----------------------------------------+


        "#});
}

#[test]
fn timeout() {
    let mut server = server::http(|_req| async move {
        tokio::time::sleep(Duration::from_secs_f32(0.5)).await;
        hyper::Response::default()
    });
    server.disable_hit_checks();

    get_command()
        .args(&["--timeout=0.1", &server.base_url()])
        .assert()
        .code(2)
        .stderr(contains("operation timed out"));
}

#[test]
fn timeout_no_limit() {
    let server = server::http(|_req| async move {
        tokio::time::sleep(Duration::from_secs_f32(0.5)).await;
        hyper::Response::default()
    });

    get_command()
        .args(&["--timeout=0", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn timeout_invalid() {
    get_command()
        .args(&["--timeout=-0.01", "--offline", ":"])
        .assert()
        .failure()
        .stderr(contains("Invalid seconds as connection timeout"));
}

#[test]
fn check_status() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(404)
            .body("".into())
            .unwrap()
    });

    get_command()
        .args(&["--check-status", &server.base_url()])
        .assert()
        .code(4)
        .stderr("");
}

#[test]
fn check_status_warning() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(501)
            .body("".into())
            .unwrap()
    });

    redirecting_command()
        .args(&["--check-status", &server.base_url()])
        .assert()
        .code(5)
        .stderr("xh: warning: HTTP 501 Not Implemented\n");
}

#[test]
fn check_status_is_implied() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(404)
            .body("".into())
            .unwrap()
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .code(4)
        .stderr("");
}

#[test]
fn check_status_is_not_implied_in_compat_mode() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(404)
            .body("".into())
            .unwrap()
    });

    get_command()
        .env("XH_HTTPIE_COMPAT_MODE", "")
        .arg(server.base_url())
        .assert()
        .code(0);
}

#[test]
fn user_password_auth() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Authorization"], "Basic dXNlcjpwYXNz");
        hyper::Response::default()
    });

    get_command()
        .args(&["--auth=user:pass", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn user_auth() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Authorization"], "Basic dXNlcjo=");
        hyper::Response::default()
    });

    get_command()
        .args(&["--auth=user:", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn bearer_auth() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Authorization"], "Bearer SomeToken");
        hyper::Response::default()
    });

    get_command()
        .args(&["--bearer=SomeToken", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn digest_auth() {
    let server = server::http(|req| async move {
        if req.headers().get("Authorization").is_none() {
            hyper::Response::builder()
                .status(401)
                .header("WWW-Authenticate", r#"Digest realm="me@xh.com", nonce="e5051361f053723a807674177fc7022f", qop="auth, auth-int", opaque="9dcf562038f1ec1c8d02f218ef0e7a4b", algorithm=MD5, stale=FALSE"#)
                .body("".into())
                .unwrap()
        } else {
            hyper::Response::builder()
                .body("authenticated".into())
                .unwrap()
        }
    });

    get_command()
        .arg("--auth-type=digest")
        .arg("--auth=ahmed:12345")
        .arg(server.base_url())
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"));

    server.assert_hits(2);
}

#[cfg(feature = "online-tests")]
#[test]
fn successful_digest_auth() {
    get_command()
        .arg("--auth-type=digest")
        .arg("--auth=ahmed:12345")
        .arg("httpbin.org/digest-auth/5/ahmed/12345")
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"));
}

#[cfg(feature = "online-tests")]
#[test]
fn unsuccessful_digest_auth() {
    get_command()
        .arg("--auth-type=digest")
        .arg("--auth=ahmed:wrongpass")
        .arg("httpbin.org/digest-auth/5/ahmed/12345")
        .assert()
        .stdout(contains("HTTP/1.1 401 Unauthorized"));
}

#[test]
fn digest_auth_with_redirection() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/login_page" => {
                if req.headers().get("Authorization").is_none() {
                    hyper::Response::builder()
                        .status(401)
                        .header("WWW-Authenticate", r#"Digest realm="me@xh.com", nonce="e5051361f053723a807674177fc7022f", qop="auth, auth-int", opaque="9dcf562038f1ec1c8d02f218ef0e7a4b", algorithm=MD5, stale=FALSE"#)
                        .header("date", "N/A")
                        .body("".into())
                        .unwrap()
                } else {
                    hyper::Response::builder()
                        .status(302)
                        .header("location", "/admin_page")
                        .header("date", "N/A")
                        .body("authentication successful, redirecting...".into())
                        .unwrap()
                }
            }
            "/admin_page" => {
                if req.headers().get("Authorization").is_none() {
                    hyper::Response::builder()
                        .header("date", "N/A")
                        .body("admin page".into())
                        .unwrap()
                } else {
                    hyper::Response::builder()
                        .status(401)
                        .body("unauthorized".into())
                        .unwrap()
                }
            }
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .env("XH_TEST_DIGEST_AUTH_CNONCE", "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ")
        .arg("--auth-type=digest")
        .arg("--auth=ahmed:12345")
        .arg("--follow")
        .arg("--verbose")
        .arg(server.url("/login_page"))
        .assert()
        .stdout(indoc! {r#"
            GET /login_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 401 Unauthorized
            Content-Length: 0
            Date: N/A
            Www-Authenticate: Digest realm="me@xh.com", nonce="e5051361f053723a807674177fc7022f", qop="auth, auth-int", opaque="9dcf562038f1ec1c8d02f218ef0e7a4b", algorithm=MD5, stale=FALSE



            GET /login_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Authorization: Digest username="ahmed", realm="me@xh.com", nonce="e5051361f053723a807674177fc7022f", uri="/login_page", qop=auth, nc=00000001, cnonce="f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ", response="894fd5ee1dcc702df7e4a6abed37fd56", opaque="9dcf562038f1ec1c8d02f218ef0e7a4b", algorithm=MD5
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 41
            Date: N/A
            Location: /admin_page

            authentication successful, redirecting...

            GET /admin_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 10
            Date: N/A

            admin page
        "#});

    server.assert_hits(3);
}

#[test]
fn netrc_env_user_password_auth() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Authorization"], "Basic dXNlcjpwYXNz");
        hyper::Response::default()
    });

    let mut netrc = NamedTempFile::new().unwrap();
    writeln!(
        netrc,
        "machine {}\nlogin user\npassword pass",
        server.host()
    )
    .unwrap();

    get_command()
        .env("NETRC", netrc.path())
        .arg(server.base_url())
        .assert()
        .success();
}

#[test]
fn netrc_env_no_bearer_auth_unless_specified() {
    // Test that we don't pass an authorization header if the .netrc contains no username,
    // and the --auth-type=bearer flag isn't explicitly specified.
    let server = server::http(|req| async move {
        assert!(req.headers().get("Authorization").is_none());
        hyper::Response::default()
    });

    let mut netrc = NamedTempFile::new().unwrap();
    writeln!(netrc, "machine {}\npassword pass", server.host()).unwrap();

    get_command()
        .env("NETRC", netrc.path())
        .arg(server.base_url())
        .assert()
        .success();
}

#[test]
fn netrc_env_auth_type_bearer() {
    // If we're using --auth-type=bearer, test that it's properly sent with a .netrc that
    // contains only a password and no username.
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["Authorization"], "Bearer pass");
        hyper::Response::default()
    });

    let mut netrc = NamedTempFile::new().unwrap();
    writeln!(netrc, "machine {}\npassword pass", server.host()).unwrap();

    get_command()
        .env("NETRC", netrc.path())
        .arg(server.base_url())
        .arg("--auth-type=bearer")
        .assert()
        .success();
}

#[test]
fn netrc_file_user_password_auth() {
    for netrc_file in &[".netrc", "_netrc"] {
        let server = server::http(|req| async move {
            assert_eq!(req.headers()["Authorization"], "Basic dXNlcjpwYXNz");
            hyper::Response::default()
        });

        let homedir = TempDir::new().unwrap();
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
            .env_remove("NETRC")
            .arg(server.base_url())
            .assert()
            .success();

        drop(netrc);
        homedir.close().unwrap();
    }
}

fn get_proxy_command(
    protocol_to_request: &str,
    protocol_to_proxy: &str,
    proxy_url: &str,
) -> Command {
    let mut cmd = get_command();
    cmd.arg("--check-status")
        .arg(format!("--proxy={}:{}", protocol_to_proxy, proxy_url))
        .arg("GET")
        .arg(format!("{}://example.test/get", protocol_to_request));
    cmd
}

#[test]
fn proxy_http_proxy() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "GET");
        assert_eq!(req.headers()["host"], "example.test");
        hyper::Response::default()
    });

    get_proxy_command("http", "http", &server.base_url())
        .assert()
        .success();
}

#[test]
fn proxy_https_proxy() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "CONNECT");
        hyper::Response::builder()
            .status(502)
            .body("".into())
            .unwrap()
    });

    get_proxy_command("https", "https", &server.base_url())
        .assert()
        .stderr(contains("unsuccessful tunnel"))
        .failure();
}

#[test]
fn proxy_http_all_proxy() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "GET");
        hyper::Response::builder()
            .status(502)
            .body("".into())
            .unwrap()
    });

    get_proxy_command("http", "all", &server.base_url())
        .assert()
        .stdout(contains("HTTP/1.1 502 Bad Gateway"))
        .failure();
}

#[test]
fn proxy_https_all_proxy() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "CONNECT");
        hyper::Response::builder()
            .status(502)
            .body("".into())
            .unwrap()
    });

    get_proxy_command("https", "all", &server.base_url())
        .assert()
        .stderr(contains("unsuccessful tunnel"))
        .failure();
}

#[test]
fn last_supplied_proxy_wins() {
    let mut first_server = server::http(|req| async move {
        assert_eq!(req.headers()["host"], "example.test");
        hyper::Response::builder()
            .status(500)
            .body("".into())
            .unwrap()
    });

    let second_server = server::http(|req| async move {
        assert_eq!(req.headers()["host"], "example.test");
        hyper::Response::builder()
            .status(200)
            .body("".into())
            .unwrap()
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

    first_server.disable_hit_checks();
    first_server.assert_hits(0);
    second_server.assert_hits(1);
}

#[test]
fn proxy_multiple_valid_proxies() {
    let mut cmd = get_command();
    cmd.arg("--offline")
        .arg("--proxy=http:https://127.0.0.1:8000")
        .arg("--proxy=https:socks5://127.0.0.1:8000")
        .arg("--proxy=all:http://127.0.0.1:8000")
        .arg("GET")
        .arg("http://httpbin.org/get");

    cmd.assert().success();
}

#[cfg(feature = "online-tests")]
#[test]
fn verify_default_yes() {
    get_command()
        .args(&["-v", "https://self-signed.badssl.com"])
        .assert()
        .failure()
        .stdout(contains("GET / HTTP/1.1"))
        .stderr(contains("UnknownIssuer"));
}

#[cfg(feature = "online-tests")]
#[test]
fn verify_explicit_yes() {
    get_command()
        .args(&["-v", "--verify=yes", "https://self-signed.badssl.com"])
        .assert()
        .failure()
        .stdout(contains("GET / HTTP/1.1"))
        .stderr(contains("UnknownIssuer"));
}

#[cfg(feature = "online-tests")]
#[test]
fn verify_no() {
    get_command()
        .args(&["-v", "--verify=no", "https://self-signed.badssl.com"])
        .assert()
        .stdout(contains("GET / HTTP/1.1"))
        .stdout(contains("HTTP/1.1 200 OK"))
        .stderr(predicates::str::is_empty());
}

#[cfg(feature = "online-tests")]
#[test]
fn verify_valid_file() {
    get_command()
        .arg("-v")
        .arg("--verify=tests/fixtures/certs/wildcard-self-signed.pem")
        .arg("https://self-signed.badssl.com")
        .assert()
        .stdout(contains("GET / HTTP/1.1"))
        .stdout(contains("HTTP/1.1 200 OK"))
        .stderr(predicates::str::is_empty());
}

// This test may fail if https://github.com/seanmonstar/reqwest/issues/1260 is fixed
// If that happens make sure to remove the warning, not just this test
#[cfg(all(feature = "native-tls", feature = "online-tests"))]
#[test]
fn verify_valid_file_native_tls() {
    get_command()
        .arg("--native-tls")
        .arg("--verify=tests/fixtures/certs/wildcard-self-signed.pem")
        .arg("https://self-signed.badssl.com")
        .assert()
        .stderr(contains("Custom CA bundles with native-tls are broken"));
}

#[cfg(feature = "online-tests")]
#[test]
fn cert_without_key() {
    get_command()
        .args(&["-v", "https://client.badssl.com"])
        .assert()
        .stdout(contains("400 No required SSL certificate was sent"))
        .stderr(predicates::str::is_empty());
}

#[cfg(feature = "online-tests")]
#[test]
fn use_ipv4() {
    get_command()
        .args(&["https://api64.ipify.org", "--body", "--ipv4"])
        .assert()
        .stdout(function(|output: &str| {
            IpAddr::from_str(output.trim()).unwrap().is_ipv4()
        }))
        .stderr(predicates::str::is_empty());
}

// real use ipv6
#[cfg(all(feature = "ipv6-tests", feature = "online-tests"))]
#[test]
fn use_ipv6() {
    get_command()
        .args(&["https://api64.ipify.org", "--body", "--ipv6"])
        .assert()
        .stdout(function(|output: &str| {
            IpAddr::from_str(output.trim()).unwrap().is_ipv6()
        }))
        .stderr(predicates::str::is_empty());
}

#[cfg(feature = "online-tests")]
#[ignore = "certificate expired (I think)"]
#[test]
fn cert_with_key() {
    get_command()
        .arg("-v")
        .arg("--cert=tests/fixtures/certs/client.badssl.com.crt")
        .arg("--cert-key=tests/fixtures/certs/client.badssl.com.key")
        .arg("https://client.badssl.com")
        .assert()
        .stdout(contains("HTTP/1.1 200 OK"))
        .stdout(contains("client-authenticated"))
        .stderr(predicates::str::is_empty());
}

#[cfg(all(feature = "native-tls", feature = "online-tests"))]
#[test]
fn cert_with_key_native_tls() {
    get_command()
        .arg("--native-tls")
        .arg("--cert=tests/fixtures/certs/client.badssl.com.crt")
        .arg("--cert-key=tests/fixtures/certs/client.badssl.com.key")
        .arg("https://client.badssl.com")
        .assert()
        .failure()
        .stderr(contains(
            "Client certificates are not supported for native-tls",
        ));
}

#[cfg(not(feature = "native-tls"))]
#[test]
fn native_tls_flag_disabled() {
    get_command()
        .args(&["--native-tls", ":"])
        .assert()
        .failure()
        .stderr(contains("built without native-tls support"));
}

#[cfg(all(not(feature = "native-tls"), feature = "online-tests"))]
#[test]
fn improved_https_ip_error_no_support() {
    get_command()
        .arg("https://1.1.1.1")
        .assert()
        .failure()
        .stderr(contains("rustls does not support"))
        .stderr(contains("building with the `native-tls` feature"));
}

#[cfg(all(feature = "native-tls", feature = "online-tests"))]
#[test]
fn native_tls_works() {
    get_command()
        .args(&["--native-tls", "https://example.org"])
        .assert()
        .success();
}

#[cfg(all(feature = "native-tls", feature = "online-tests"))]
#[test]
fn improved_https_ip_error_with_support() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(301)
            .header("Location", "https://1.1.1.1")
            .body("Moved Permanently".into())
            .unwrap()
    });
    get_command()
        .args(&["--follow", &server.base_url()])
        .assert()
        .failure()
        .stderr(contains("rustls does not support"))
        .stderr(contains("using the --native-tls flag"));
}

#[cfg(feature = "native-tls")]
#[test]
fn auto_nativetls() {
    get_command()
        .args(&["--offline", "https://1.1.1.1"])
        .assert()
        .success()
        .stderr(contains("native-tls will be enabled"));
}

#[cfg(feature = "online-tests")]
#[test]
fn good_tls_version() {
    get_command()
        .arg("--ssl=tls1.2")
        .arg("https://tls-v1-2.badssl.com:1012/")
        .assert()
        .success();
}

#[cfg(all(feature = "native-tls", feature = "online-tests"))]
#[test]
fn good_tls_version_nativetls() {
    get_command()
        .arg("--ssl=tls1.2")
        .arg("--native-tls")
        .arg("https://tls-v1-2.badssl.com:1012/")
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
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["content-type"], "application/json");
        assert_eq!(req.headers()["accept"], "application/json, */*;q=0.5");
        hyper::Response::default()
    });

    get_command()
        .args(&["--json", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn forced_form() {
    let server = server::http(|req| async move {
        assert_eq!(
            req.headers()["content-type"],
            "application/x-www-form-urlencoded"
        );
        hyper::Response::default()
    });
    get_command()
        .args(&["--form", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn forced_multipart() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.headers().get("content-type").is_some(), true);
        assert_eq!(req.body_as_string().await, "");
        hyper::Response::default()
    });
    get_command()
        .args(&["--multipart", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn formatted_json_output() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("content-type", "application/json")
            .body(r#"{"":0}"#.into())
            .unwrap()
    });
    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            {
                "": 0
            }


        "#});
}

#[test]
fn inferred_json_output() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("content-type", "text/plain")
            .body(r#"{"":0}"#.into())
            .unwrap()
    });
    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            {
                "": 0
            }


        "#});
}

#[test]
fn inferred_json_javascript_output() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("content-type", "application/javascript")
            .body(r#"{"":0}"#.into())
            .unwrap()
    });
    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            {
                "": 0
            }


        "#});
}

#[test]
fn inferred_nonjson_output() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("content-type", "text/plain")
            // Trailing comma makes it invalid JSON, though formatting would still work
            .body(r#"{"":0,}"#.into())
            .unwrap()
    });
    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            {"":0,}
        "#});
}

#[test]
fn noninferred_json_output() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            // Valid JSON, but not declared as text
            .header("content-type", "application/octet-stream")
            .body(r#"{"":0}"#.into())
            .unwrap()
    });
    get_command()
        .args(&["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            {"":0}
        "#});
}

#[test]
fn empty_body_defaults_to_get() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "GET");
        assert_eq!(req.body_as_string().await, "");
        hyper::Response::default()
    });

    get_command().arg(server.base_url()).assert().success();
}

#[test]
fn non_empty_body_defaults_to_post() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.body_as_string().await, "{\"x\":4}");
        hyper::Response::default()
    });

    get_command()
        .args(&[&server.base_url(), "x:=4"])
        .assert()
        .success();
}

#[test]
fn empty_raw_body_defaults_to_post() {
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.body_as_string().await, "");
        hyper::Response::default()
    });

    redirecting_command()
        .arg(server.base_url())
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn body_from_stdin() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, "body from stdin");
        hyper::Response::default()
    });

    redirecting_command()
        .arg(server.base_url())
        .write_stdin("body from stdin")
        .assert()
        .success();
}

#[test]
fn body_from_raw() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, "body from raw");
        hyper::Response::default()
    });

    get_command()
        .args(&["--raw=body from raw", &server.base_url()])
        .assert()
        .success();
}

#[test]
fn mixed_stdin_request_items() {
    redirecting_command()
        .args(&["--offline", ":", "x=3"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains(
            "Request body (from stdin) and request data (key=value) cannot be mixed",
        ));
}

#[test]
fn mixed_stdin_raw() {
    redirecting_command()
        .args(&["--offline", "--raw=hello", ":"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains(
            "Request body from stdin and --raw cannot be mixed",
        ));
}

#[test]
fn mixed_raw_request_items() {
    get_command()
        .args(&["--offline", "--raw=hello", ":", "x=3"])
        .assert()
        .failure()
        .stderr(contains(
            "Request body (from --raw) and request data (key=value) cannot be mixed",
        ));
}

#[test]
fn multipart_stdin() {
    redirecting_command()
        .args(&["--offline", "--multipart", ":"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("Cannot build a multipart request body from stdin"));
}

#[test]
fn multipart_raw() {
    get_command()
        .args(&["--offline", "--raw=hello", "--multipart", ":"])
        .assert()
        .failure()
        .stderr(contains("Cannot build a multipart request body from --raw"));
}

#[test]
fn default_json_for_raw_body() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["content-type"], "application/json");
        hyper::Response::default()
    });
    redirecting_command()
        .arg(server.base_url())
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn multipart_file_upload() {
    let server = server::http(|req| async move {
        // This test may be fragile, it's conceivable that the headers will become
        // lowercase in the future
        // (so if this breaks all of a sudden, check that first)
        let body = req.body_as_string().await;
        assert!(body.contains("Hello world"));
        assert!(body.contains(concat!(
            "Content-Disposition: form-data; name=\"x\"; filename=\"input.txt\"\r\n",
            "\r\n",
            "Hello world\n"
        )));
        assert!(body.contains(concat!(
            "Content-Disposition: form-data; name=\"y\"; filename=\"foobar.htm\"\r\n",
            "Content-Type: text/html\r\n",
            "\r\n",
            "Hello world\n",
        )));

        hyper::Response::default()
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
}

#[test]
fn body_from_file() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["content-type"], "text/plain");
        assert_eq!(req.body_as_string().await, "Hello world\n");
        hyper::Response::default()
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
}

#[test]
fn body_from_file_with_explicit_mimetype() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["content-type"], "image/png");
        assert_eq!(req.body_as_string().await, "Hello world\n");
        hyper::Response::default()
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
}

#[test]
fn body_from_file_with_fallback_mimetype() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["content-type"], "application/json");
        assert_eq!(req.body_as_string().await, "Hello world\n");
        hyper::Response::default()
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
}

#[test]
fn no_double_file_body() {
    get_command()
        .args(&[":", "@foo", "@bar"])
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
        .args(&["--offline", ":"])
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
        .args(&["--offline", ":", "x:=3"])
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
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, r#"{"name":"ali","age":24}"#);
        hyper::Response::default()
    });

    get_command()
        .args(&["get", &server.base_url(), "name=ali", "age:=24"])
        .assert()
        .success();
}

#[test]
fn data_field_from_file() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, r#"{"ids":"[1,2,3]"}"#);
        hyper::Response::default()
    });

    let mut text_file = NamedTempFile::new().unwrap();
    write!(text_file, "[1,2,3]").unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("ids=@{}", text_file.path().to_string_lossy()))
        .assert()
        .success();
}

#[test]
fn data_field_from_file_in_form_mode() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, r#"message=hello+world"#);
        hyper::Response::default()
    });

    let mut text_file = NamedTempFile::new().unwrap();
    write!(text_file, "hello world").unwrap();

    get_command()
        .arg(server.base_url())
        .arg("--form")
        .arg(format!("message=@{}", text_file.path().to_string_lossy()))
        .assert()
        .success();
}

#[test]
fn json_field_from_file() {
    let server = server::http(|req| async move {
        assert_eq!(req.body_as_string().await, r#"{"ids":[1,2,3]}"#);
        hyper::Response::default()
    });

    let mut json_file = NamedTempFile::new().unwrap();
    writeln!(json_file, "[1,2,3]").unwrap();

    get_command()
        .arg(server.base_url())
        .arg(format!("ids:=@{}", json_file.path().to_string_lossy()))
        .assert()
        .success();
}

#[test]
fn can_unset_default_headers() {
    get_command()
        .args(&[":", "user-agent:", "--offline"])
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
        .args(&[":", "hello:world", "goodby:world", "goodby:", "--offline"])
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
        .args(&[":", "hello:", "hello:world", "--offline"])
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
fn named_sessions() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("set-cookie", "cook1=one; Path=/")
            .body("".into())
            .unwrap()
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

    server.assert_hits(1);

    let path_to_session = config_dir.path().join::<std::path::PathBuf>(
        [
            "sessions",
            &format!("127.0.0.1_{}", server.port()),
            &format!("{}.json", random_name),
        ]
        .iter()
        .collect(),
    );

    let session_content = fs::read_to_string(path_to_session).unwrap();

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
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("set-cookie", "cook1=one")
            .body("".into())
            .unwrap()
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

    server.assert_hits(1);

    let session_content = fs::read_to_string(path_to_session).unwrap();

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
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("set-cookie", "lang=en")
            .body("".into())
            .unwrap()
    });

    let session_file = NamedTempFile::new().unwrap();
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
        serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(session_file.path()).unwrap()
        )
        .unwrap(),
        old_session_content
    );
}

#[test]
fn session_files_are_created_in_read_only_mode() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("set-cookie", "lang=ar")
            .body("".into())
            .unwrap()
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

    let session_content = fs::read_to_string(path_to_session).unwrap();
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
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("set-cookie", "lang=en")
            .body("".into())
            .unwrap()
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
    fs::create_dir_all(path_to_session.parent().unwrap()).unwrap();
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
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(path_to_session).unwrap())
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
    let past_timestamp = 1_114_425_967; // 2005-04-25

    let session_file = NamedTempFile::new().unwrap();

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

    let session_content = fs::read_to_string(session_file.path()).unwrap();
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

fn cookies_are_equal(c1: &str, c2: &str) -> bool {
    HashSet::<_>::from_iter(c1.split(';').map(str::trim))
        == HashSet::<_>::from_iter(c2.split(';').map(str::trim))
}

#[test]
fn cookies_override_each_other_in_the_correct_order() {
    // Cookies storage priority is: Server response > Command line request > Session file
    // See https://httpie.io/docs#cookie-storage-behaviour
    let server = server::http(|req| async move {
        assert!(cookies_are_equal(
            req.headers()["cookie"].to_str().unwrap(),
            "lang=fr; cook1=two; cook2=two"
        ));
        hyper::Response::builder()
            .header("set-cookie", "lang=en")
            .header("set-cookie", "cook1=one")
            .body("".into())
            .unwrap()
    });

    let session_file = NamedTempFile::new().unwrap();

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

    server.assert_hits(1);

    let session_content = fs::read_to_string(session_file.path()).unwrap();
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
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["authorization"], "Basic dXNlcjpwYXNz");
        hyper::Response::default()
    });

    let session_file = NamedTempFile::new().unwrap();

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
}

#[test]
fn bearer_auth_from_session_is_used() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["authorization"], "Bearer secret-token");
        hyper::Response::default()
    });

    let session_file = NamedTempFile::new().unwrap();

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
}

#[test]
fn auth_netrc_is_not_persisted_in_session() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()["authorization"], "Basic dXNlcjpwYXNz");
        hyper::Response::default()
    });

    let mut path_to_session = std::env::temp_dir();
    let file_name = random_string();
    path_to_session.push(file_name);
    assert_eq!(path_to_session.exists(), false);

    let mut netrc = NamedTempFile::new().unwrap();
    writeln!(
        netrc,
        "machine {}\nlogin user\npassword pass",
        server.host()
    )
    .unwrap();

    get_command()
        .env("NETRC", netrc.path())
        .arg(server.base_url())
        .arg("hello:world")
        .arg(format!("--session={}", path_to_session.to_string_lossy()))
        .assert()
        .success();

    server.assert_hits(1);

    let session_content = fs::read_to_string(path_to_session).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session_content).unwrap(),
        serde_json::json!({
            "__meta__": {
                "about": "xh session file",
                "xh": "0.0.0"
            },
            "auth": { "type": null, "raw_auth": null },
            "cookies": {},
            "headers": {
                "hello": "world"
            }
        })
    );
}

#[test]
fn print_intermediate_requests_and_responses() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/first_page" => hyper::Response::builder()
                .status(302)
                .header("Date", "N/A")
                .header("Location", "/second_page")
                .body("redirecting...".into())
                .unwrap(),
            "/second_page" => hyper::Response::builder()
                .header("Date", "N/A")
                .body("final destination".into())
                .unwrap(),
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .args(&[&server.url("/first_page"), "--follow", "--verbose", "--all"])
        .assert()
        .stdout(indoc! {r#"
            GET /first_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: /second_page

            redirecting...

            GET /second_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#});
}

#[test]
fn history_print() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/first_page" => hyper::Response::builder()
                .status(302)
                .header("Date", "N/A")
                .header("Location", "/second_page")
                .body("redirecting...".into())
                .unwrap(),
            "/second_page" => hyper::Response::builder()
                .header("Date", "N/A")
                .body("final destination".into())
                .unwrap(),
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .arg(server.url("/first_page"))
        .arg("--follow")
        .arg("--print=HhBb")
        .arg("--history-print=Hh")
        .arg("--all")
        .assert()
        .stdout(indoc! {r#"
            GET /first_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: /second_page

            GET /second_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br
            Connection: keep-alive
            Host: http.mock
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#});
}

#[test]
fn max_redirects_is_enforced() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .status(302)
            .header("Date", "N/A")
            .header("Location", "/") // infinite redirect loop
            .body("redirecting...".into())
            .unwrap()
    });

    get_command()
        .args(&[&server.base_url(), "--follow", "--max-redirects=5"])
        .assert()
        .stderr(contains("Too many redirects (--max-redirects=5)"))
        .code(6);
}

#[test]
fn method_is_changed_when_following_302_redirect() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/first_page" => {
                assert_eq!(req.method(), "POST");
                assert!(req.headers().get("Content-Length").is_some());
                assert_eq!(req.body_as_string().await, r#"{"name":"ali"}"#);
                hyper::Response::builder()
                    .status(302)
                    .header("Location", "/second_page")
                    .body("redirecting...".into())
                    .unwrap()
            }
            "/second_page" => {
                assert_eq!(req.method(), "GET");
                assert!(req.headers().get("Content-Length").is_none());
                hyper::Response::builder()
                    .body("final destination".into())
                    .unwrap()
            }
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .args(&[
            "post",
            &server.url("/first_page"),
            "--verbose",
            "--follow",
            "name=ali",
        ])
        .assert()
        .success()
        .stdout(contains("POST /first_page HTTP/1.1"))
        .stdout(contains("GET /second_page HTTP/1.1"));

    server.assert_hits(2);
}

#[test]
fn method_is_not_changed_when_following_307_redirect() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/first_page" => {
                assert_eq!(req.method(), "POST");
                assert_eq!(req.body_as_string().await, r#"{"name":"ali"}"#);
                hyper::Response::builder()
                    .status(307)
                    .header("Location", "/second_page")
                    .body("redirecting...".into())
                    .unwrap()
            }
            "/second_page" => {
                assert_eq!(req.method(), "POST");
                assert_eq!(req.body_as_string().await, r#"{"name":"ali"}"#);
                hyper::Response::builder()
                    .body("final destination".into())
                    .unwrap()
            }
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .args(&[
            "post",
            &server.url("/first_page"),
            "--verbose",
            "--follow",
            "name=ali",
        ])
        .assert()
        .success()
        .stdout(contains("POST /first_page HTTP/1.1"))
        .stdout(contains("POST /second_page HTTP/1.1"));

    server.assert_hits(2);
}

#[test]
fn sensitive_headers_are_removed_after_cross_domain_redirect() {
    let server1 = server::http(|req| async move {
        assert!(req.headers().get("Authorization").is_none());
        assert!(req.headers().get("Hello").is_some());
        hyper::Response::builder()
            .header("Date", "N/A")
            .body("final destination".into())
            .unwrap()
    });

    let server1_base_url = server1.base_url();
    let server2 = server::http(move |req| {
        let server1_base_url = server1_base_url.clone();
        async move {
            assert!(req.headers().get("Authorization").is_some());
            assert!(req.headers().get("Hello").is_some());
            hyper::Response::builder()
                .status(302)
                .header("Location", server1_base_url)
                .body("redirecting...".into())
                .unwrap()
        }
    });

    get_command()
        .arg(server2.base_url())
        .arg("--follow")
        .arg("--auth=user:pass")
        .arg("hello:world")
        .assert()
        .success();

    server1.assert_hits(1);
    server2.assert_hits(1);
}

#[test]
fn request_body_is_buffered_for_307_redirect() {
    let server = server::http(|req| async move {
        match req.uri().path() {
            "/first_page" => hyper::Response::builder()
                .status(307)
                .header("Location", "/second_page")
                .body("redirecting...".into())
                .unwrap(),
            "/second_page" => {
                assert_eq!(req.body_as_string().await, "hello world\n");
                hyper::Response::builder()
                    .body("final destination".into())
                    .unwrap()
            }
            _ => panic!("unknown path"),
        }
    });

    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "hello world").unwrap();

    get_command()
        .arg(server.url("/first_page"))
        .arg("--follow")
        .arg("--all")
        .arg("--print=Hh") // prevent Printer from buffering the request body by not using --verbose
        .arg(format!("@{}", file.path().to_string_lossy()))
        .assert()
        .success()
        .stdout(contains("POST /second_page HTTP/1.1"));

    server.assert_hits(2);
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
        .args(&[":", "--offline"])
        .assert()
        .stderr(contains("Unable to parse config file"))
        .success();
}

#[cfg(feature = "online-tests")]
#[test]
fn http1_0() {
    get_command()
        .args(&["--print=hH", "--http-version=1.0", "https://www.google.com"])
        .assert()
        .success()
        .stdout(contains("GET / HTTP/1.0"))
        // Some servers i.e nginx respond with HTTP/1.1 to HTTP/1.0 requests, see https://serverfault.com/questions/442960/nginx-ignoring-clients-http-1-0-request-and-respond-by-http-1-1
        // Fortunately, https://www.google.com is not one of those.
        .stdout(contains("HTTP/1.0 200 OK"));
}

#[cfg(feature = "online-tests")]
#[test]
fn http1_1() {
    get_command()
        .args(&["--print=hH", "--http-version=1.1", "https://www.google.com"])
        .assert()
        .success()
        .stdout(contains("GET / HTTP/1.1"))
        .stdout(contains("HTTP/1.1 200 OK"));
}

#[cfg(feature = "online-tests")]
#[test]
fn http2() {
    get_command()
        .args(&["--print=hH", "--http-version=2", "https://www.google.com"])
        .assert()
        .success()
        .stdout(contains("GET / HTTP/2.0"))
        .stdout(contains("HTTP/2.0 200 OK"));
}

#[test]
fn override_response_charset() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(b"\xe9".as_ref().into())
            .unwrap()
    });

    get_command()
        .arg("--print=b")
        .arg("--response-charset=latin1")
        .arg(server.base_url())
        .assert()
        .stdout("é\n");
}

#[test]
fn override_response_mime() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body("{\"status\": \"ok\"}".into())
            .unwrap()
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
}

#[test]
fn omit_response_body() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("date", "N/A")
            .body("Hello!".into())
            .unwrap()
    });

    get_command()
        .arg("--print=h")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Length: 6
            Date: N/A

        "#});
}

#[test]
fn encoding_detection() {
    fn case(
        content_type: &'static str,
        body: &'static (impl AsRef<[u8]> + ?Sized),
        output: &'static str,
    ) {
        let body = body.as_ref();
        let server = server::http(move |_| async move {
            hyper::Response::builder()
                .header("Content-Type", content_type)
                .body(body.into())
                .unwrap()
        });

        get_command()
            .arg("--print=b")
            .arg(server.base_url())
            .assert()
            .stdout(output);

        get_command()
            .arg("--print=b")
            .arg("--stream")
            .arg(server.base_url())
            .assert()
            .stdout(output);

        server.assert_hits(2);
    }

    // UTF-8 is a typical fallback
    case("text/plain", "é", "é\n");

    // But headers take precedence
    case("text/html; charset=latin1", "é", "Ã©\n");

    // As do BOMs
    case("text/html", b"\xFF\xFEa\0b\0", "ab\n");

    // windows-1252 is another common fallback
    case("text/plain", b"\xFF", "ÿ\n");

    // BOMs are stripped
    case("text/plain", b"\xFF\xFEa\0b\0", "ab\n");
    case("text/plain; charset=UTF-16", b"\xFF\xFEa\0b\0", "ab\n");
    case("text/plain; charset=UTF-16LE", b"\xFF\xFEa\0b\0", "ab\n");
    case("text/plain", b"\xFE\xFF\0a\0b", "ab\n");
    case("text/plain; charset=UTF-16BE", b"\xFE\xFF\0a\0b", "ab\n");

    // ...unless they're for a different encoding
    case(
        "text/plain; charset=UTF-16LE",
        b"\xFE\xFFa\0b\0",
        "\u{FFFE}ab\n",
    );
    case(
        "text/plain; charset=UTF-16BE",
        b"\xFF\xFE\0a\0b",
        "\u{FFFE}ab\n",
    );

    // Binary content is detected
    case("application/octet-stream", "foo\0bar", BINARY_SUPPRESSOR);

    // (even for non-ASCII-compatible encodings)
    case("text/plain; charset=UTF-16", "\0\0", BINARY_SUPPRESSOR);
}

#[test]
fn tilde_expanded_in_request_items() {
    let homedir = TempDir::new().unwrap();

    std::fs::write(homedir.path().join("secret_key.txt"), "sxemfalm.....").unwrap();
    get_command()
        .env("HOME", homedir.path())
        .env("XH_TEST_MODE_WIN_HOME_DIR", homedir.path())
        .args(&["--offline", ":", "key=@~/secret_key.txt"])
        .assert()
        .stdout(contains("sxemfalm....."))
        .success();

    std::fs::write(homedir.path().join("ids.json"), "[102,111,164]").unwrap();
    get_command()
        .env("HOME", homedir.path())
        .env("XH_TEST_MODE_WIN_HOME_DIR", homedir.path())
        .args(&["--offline", "--pretty=none", ":", "ids:=@~/ids.json"])
        .assert()
        .stdout(contains("[102,111,164]"))
        .success();

    std::fs::write(homedir.path().join("moby-dick.txt"), "Call me Ishmael.").unwrap();
    get_command()
        .env("HOME", homedir.path())
        .env("XH_TEST_MODE_WIN_HOME_DIR", homedir.path())
        .args(&["--offline", "--form", ":", "content@~/moby-dick.txt"])
        .assert()
        .stdout(contains("Call me Ishmael."))
        .success();

    std::fs::write(homedir.path().join("random_file"), "random data").unwrap();
    get_command()
        .env("HOME", homedir.path())
        .env("XH_TEST_MODE_WIN_HOME_DIR", homedir.path())
        .args(&["--offline", ":", "@~/random_file"])
        .assert()
        .stdout(contains("random data"))
        .success();
}

#[test]
fn gzip() {
    let server = server::http(|_req| async move {
        let compressed_bytes = fs::read("./tests/fixtures/responses/hello_world.gz").unwrap();
        hyper::Response::builder()
            .header("date", "N/A")
            .header("content-encoding", "gzip")
            .body(compressed_bytes.into())
            .unwrap()
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Encoding: gzip
            Content-Length: 48
            Date: N/A

            Hello world

        "#});
}

#[test]
fn deflate() {
    let server = server::http(|_req| async move {
        let compressed_bytes = fs::read("./tests/fixtures/responses/hello_world.zz").unwrap();
        hyper::Response::builder()
            .header("date", "N/A")
            .header("content-encoding", "deflate")
            .body(compressed_bytes.into())
            .unwrap()
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Encoding: deflate
            Content-Length: 20
            Date: N/A

            Hello world

        "#});
}

#[test]
fn brotli() {
    let server = server::http(|_req| async move {
        let compressed_bytes = fs::read("./tests/fixtures/responses/hello_world.br").unwrap();
        hyper::Response::builder()
            .header("date", "N/A")
            .header("content-encoding", "br")
            .body(compressed_bytes.into())
            .unwrap()
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Encoding: br
            Content-Length: 17
            Date: N/A

            Hello world

        "#});
}

#[test]
fn empty_response_with_content_encoding() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("date", "N/A")
            .header("content-encoding", "gzip")
            .body("".into())
            .unwrap()
    });

    get_command()
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Encoding: gzip
            Content-Length: 0
            Date: N/A


        "#});
}

#[test]
fn empty_response_with_content_encoding_and_content_length() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("date", "N/A")
            .header("content-encoding", "gzip")
            .header("content-length", "100")
            .body("".into())
            .unwrap()
    });

    get_command()
        .arg("head")
        .arg(server.base_url())
        .assert()
        .stdout(indoc! {r#"
            HTTP/1.1 200 OK
            Content-Encoding: gzip
            Content-Length: 100
            Date: N/A


        "#});
}

#[test]
fn check_non_get_redirect_warning() {
    get_command()
        .args(&["--follow", "--curl", "POST", "http://example.com"])
        .assert()
        .stderr(contains("Using a combination of -X/--request and -L/--location which may cause unintended side effects."));
}
