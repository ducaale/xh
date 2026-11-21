use indoc::indoc;

use crate::prelude::*;

#[test]
fn xml_pretty_printing() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body(r#"<?xml version="1.0"?><catalog><book id="1"><author>Gambardella</author><title>XML Developer Guide</title></book></catalog>"#.into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <?xml version="1.0"?>
            <catalog>
              <book id="1">
                <author>Gambardella</author>
                <title>XML Developer Guide</title>
              </book>
            </catalog>
        "#});
}

#[test]
fn xml_pretty_printing_text_xml_content_type() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "text/xml")
            .body("<root><a>text</a></root>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <root>
              <a>text</a>
            </root>
        "#});
}

#[test]
fn xml_format_disabled() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root><a>text</a></root>".into())
            .unwrap()
    });
    get_command()
        .args([
            "--print=b",
            "--format-options=xml.format:false",
            &server.base_url(),
        ])
        .assert()
        .stdout("<root><a>text</a></root>\n");
}

#[test]
fn xml_custom_indent() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root><a>text</a></root>".into())
            .unwrap()
    });
    get_command()
        .args([
            "--print=b",
            "--format-options=xml.indent:6",
            &server.base_url(),
        ])
        .assert()
        .stdout(indoc! {r#"
            <root>
                  <a>text</a>
            </root>
        "#});
}

#[test]
fn xml_invalid_falls_back_gracefully() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<a><b>text</a></b>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout("<a><b>text</a></b>\n");
}

#[test]
fn xml_declaration_preserved() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body(r#"<?xml version="1.0" encoding="UTF-8"?><root><a>text</a></root>"#.into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <root>
              <a>text</a>
            </root>
        "#});
}

#[test]
fn xml_pretty_none() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root><a>text</a></root>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", "--pretty=none", &server.base_url()])
        .assert()
        .stdout("<root><a>text</a></root>\n");
}

#[test]
fn xml_streaming_skips_formatting() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root><a>text</a><b>more</b></root>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", "--stream", &server.base_url()])
        .assert()
        .stdout("<root><a>text</a><b>more</b></root>\n");
}

#[test]
fn xml_mixed_content_preserved() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root><p>Hello <b>world</b> end</p></root>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <root>
              <p>Hello <b>world</b> end</p>
            </root>
        "#});
}

#[test]
fn xml_already_formatted() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xml")
            .body("<root>\n  <a>\n    <b>text</b>\n  </a>\n</root>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <root>
              <a>
                <b>text</b>
              </a>
            </root>
        "#});
}

#[test]
fn xml_xhtml_content_type() {
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Type", "application/xhtml+xml")
            .body("<html><body><p>hello</p></body></html>".into())
            .unwrap()
    });
    get_command()
        .args(["--print=b", &server.base_url()])
        .assert()
        .stdout(indoc! {r#"
            <html>
              <body>
                <p>hello</p>
              </body>
            </html>
        "#});
}
