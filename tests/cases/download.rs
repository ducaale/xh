use std::fs;

use tempfile::tempdir;

use crate::prelude::*;

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
        .args([&server.base_url(), "--download", "accept-encoding:gzip"])
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
        .args(["--download", &server.url("/foo/bar/")])
        .current_dir(&dir)
        .assert()
        .success();

    get_command()
        .args(["--download", &server.url("/foo/bar/")])
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
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("foo.bar")).unwrap(),
        "file"
    );
}

#[test]
fn download_supplied_unicode_filename() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header("Content-Disposition", r#"attachment; filename="😀.bar""#)
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("😀.bar")).unwrap(),
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
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("foo bar baz")).unwrap(),
        "file"
    );
}

#[test]
fn download_filename_with_directory_traversal() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header(
                "Content-Disposition",
                r#"attachment; filename="foo/baz/bar""#,
            )
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(dir.path().join("bar")).unwrap(), "file");
}

#[cfg(windows)]
#[test]
fn download_filename_with_windows_directory_traversal() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header(
                "Content-Disposition",
                r#"attachment; filename="foo\baz\bar""#,
            )
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(dir.path().join("bar")).unwrap(), "file");
}

// TODO: test implicit download filenames
// For this we have to pretend the output is a tty
// This intersects with both #41 and #59
