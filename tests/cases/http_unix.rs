use indoc::indoc;

use crate::prelude::*;

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
        .assert()
        .success();

    server.assert_hits(3);
}

// TODO: add tests for cookies
