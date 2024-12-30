#[cfg(unix)]
use indoc::indoc;

use crate::prelude::*;

#[cfg(not(unix))]
#[test]
fn error_on_unsupported_platform() {
    use predicates::str::contains;

    get_command()
        .arg(format!("--unix-socket=/tmp/missing.sock",))
        .arg(":/index.html")
        .assert()
        .failure()
        .stderr(contains(
            "HTTP over Unix domain sockets is not supported on this platform",
        ));
}

#[cfg(unix)]
#[test]
fn json_post() {
    let server = server::http_unix(|req| async move {
        assert_eq!(req.method(), "POST");
        assert_eq!(req.headers()["Content-Type"], "application/json");
        assert_eq!(req.body_as_string().await, "{\"foo\":\"bar\"}");

        hyper::Response::builder()
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(r#"{"status":"ok"}"#.into())
            .unwrap()
    });

    get_command()
        .arg("--print=b")
        .arg("--pretty=format")
        .arg("post")
        .arg("http://example.com")
        .arg(format!(
            "--unix-socket={}",
            server.socket_path().to_string_lossy()
        ))
        .arg("foo=bar")
        .assert()
        .stdout(indoc! {r#"
            {
                "status": "ok"
            }


        "#});
}

#[cfg(unix)]
#[test]
fn redirects_stay_on_same_server() {
    let server = server::http_unix(|req| async move {
        match dbg!(req.uri().to_string().as_str()) {
            "http://example.com/first_page" => hyper::Response::builder()
                .status(302)
                .header("Date", "N/A")
                .header("Location", "http://localhost:8000/second_page")
                .body("redirecting...".into())
                .unwrap(),
            "http://localhost:8000/second_page" => hyper::Response::builder()
                .status(302)
                .header("Date", "N/A")
                .header("Location", "/third_page")
                .body("redirecting...".into())
                .unwrap(),
            "http://localhost:8000/third_page" => hyper::Response::builder()
                .header("Date", "N/A")
                .body("final destination".into())
                .unwrap(),
            _ => panic!("unknown path"),
        }
    });

    get_command()
        .arg("http://example.com/first_page")
        .arg(format!(
            "--unix-socket={}",
            server.socket_path().to_string_lossy()
        ))
        .arg("--follow")
        .arg("--verbose")
        .arg("--all")
        .assert()
        .stdout(indoc! {r#"
            GET /first_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: example.com
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: http://localhost:8000/second_page

            redirecting...

            GET /second_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: localhost:8000
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: /third_page

            redirecting...

            GET /third_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: localhost:8000
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#});

    server.assert_hits(3);
}

#[cfg(unix)]
#[test]
fn cookies_persist_across_redirects() {
    let server = server::http_unix(|req| async move {
        match req.uri().path() {
            "/first_page" => hyper::Response::builder()
                .status(302)
                .header("Date", "N/A")
                .header("Location", "/second_page")
                .header("set-cookie", "hello=world")
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
        .arg("localhost:3000/first_page")
        .arg(format!(
            "--unix-socket={}",
            server.socket_path().to_string_lossy()
        ))
        .arg("--follow")
        .arg("--verbose")
        .arg("--all")
        .assert()
        .stdout(indoc! {r#"
            GET /first_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Host: localhost:3000
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 302 Found
            Content-Length: 14
            Date: N/A
            Location: /second_page
            Set-Cookie: hello=world

            redirecting...

            GET /second_page HTTP/1.1
            Accept: */*
            Accept-Encoding: gzip, deflate, br, zstd
            Connection: keep-alive
            Cookie: hello=world
            Host: localhost:3000
            User-Agent: xh/0.0.0 (test mode)

            HTTP/1.1 200 OK
            Content-Length: 17
            Date: N/A

            final destination
        "#});
}

// TODO: add tests for connection timeout
