use std::{
    fs::{self, OpenOptions},
    io::Write,
};

use predicates::str::contains;
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
fn download_support_filename_rfc_5987() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header(
                "Content-Disposition",
                r#"attachment; filename*=UTF-8''abcd1234.txt"#,
            )
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("abcd1234.txt")).unwrap(),
        "file"
    );
}
#[test]
fn download_support_filename_rfc_5987_percent_encoded() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header(
                "Content-Disposition",
                r#"attachment; filename*=UTF-8''%E6%B5%8B%E8%AF%95.txt"#,
            )
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("测试.txt")).unwrap(),
        "file"
    );
}

#[test]
fn download_filename_star_with_high_priority() {
    let dir = tempdir().unwrap();
    let server = server::http(|_req| async move {
        hyper::Response::builder()
            .header(
                "Content-Disposition",
                r#"attachment; filename="fallback.txt"; filename*=UTF-8''%E6%B5%8B%E8%AF%95.txt"#,
            )
            .body("file".into())
            .unwrap()
    });

    get_command()
        .args(["--download", &server.base_url()])
        .current_dir(&dir)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("测试.txt")).unwrap(),
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

#[test]
fn it_can_resume_a_download() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()[hyper::header::RANGE], "bytes=5-");

        hyper::Response::builder()
            .status(206)
            .header(hyper::header::CONTENT_RANGE, "bytes 5-11/12")
            .body(" world\n".into())
            .unwrap()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello")
        .unwrap();

    get_command()
        .arg("--download")
        .arg("--continue")
        .arg("--output")
        .arg(&filename)
        .arg(server.base_url())
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&filename).unwrap(), "Hello world\n");
}

#[test]
fn it_can_resume_a_download_with_one_byte() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()[hyper::header::RANGE], "bytes=5-");

        hyper::Response::builder()
            .status(206)
            .header(hyper::header::CONTENT_RANGE, "bytes 5-5/6")
            .body("!".into())
            .unwrap()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello")
        .unwrap();

    get_command()
        .arg("--download")
        .arg("--continue")
        .arg("--output")
        .arg(&filename)
        .arg(server.base_url())
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&filename).unwrap(), "Hello!");
}

#[test]
fn it_rejects_incorrect_content_range_headers() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()[hyper::header::RANGE], "bytes=5-");

        hyper::Response::builder()
            .status(206)
            .header(hyper::header::CONTENT_RANGE, "bytes 6-10/11")
            .body("world\n".into())
            .unwrap()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello")
        .unwrap();

    get_command()
        .arg("--download")
        .arg("--continue")
        .arg("--output")
        .arg(&filename)
        .arg(server.base_url())
        .assert()
        .failure()
        .stderr(contains("Content-Range has wrong start"));
}

#[test]
fn it_refuses_to_combine_continue_and_range() {
    let server = server::http(|req| async move {
        assert_eq!(req.headers()[hyper::header::RANGE], "bytes=20-30");

        hyper::Response::builder()
            .status(206)
            .header(hyper::header::CONTENT_RANGE, "bytes 20-30/100")
            .body("lorem ipsum".into())
            .unwrap()
    });

    let dir = tempfile::tempdir().unwrap();
    let filename = dir.path().join("input.txt");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&filename)
        .unwrap()
        .write_all(b"Hello")
        .unwrap();

    get_command()
        .arg("--download")
        .arg("--continue")
        .arg("--output")
        .arg(&filename)
        .arg(server.base_url())
        .arg("Range:bytes=20-30")
        .assert()
        .success()
        .stderr(contains("warning: --continue can't be used with"));

    assert_eq!(fs::read_to_string(&filename).unwrap(), "lorem ipsum");
}
