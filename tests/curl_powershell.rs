//! Native Windows validation: cargo test --test curl_powershell -- --ignored --nocapture
//! Requires Windows PowerShell 5.1, PowerShell 7.3+, rustc, and a UTF-8-aware curl.exe.
//! No emulation, public endpoint, administrator privileges, or workflow is needed.
//! Set XH_POWERSHELL_EVIDENCE_DIR to retain JSON records of the exact bytes checked.
#![cfg(windows)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn save_evidence(name: &str, value: serde_json::Value) {
    if let Some(dir) = std::env::var_os("XH_POWERSHELL_EVIDENCE_DIR") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            Path::new(&dir).join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}

fn translation(shell: Option<&str>, url: &str, body: &str, dir: &Path) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xh"));
    command
        .env("XH_CONFIG_DIR", dir)
        .args(["--ignore-stdin", "--curl", "--timeout=5"]);
    if let Some(shell) = shell {
        command.arg(format!("--curl-shell={shell}"));
    }
    let output = success(command.args(["POST", url, "--raw", body]).output().unwrap());
    String::from_utf8(output.stdout).unwrap()
}

fn powershell(executable: &str, mode: &str, script: &str, dir: &Path) -> Command {
    let guard = if executable == "powershell.exe" {
        "if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) { throw 'Requires Windows PowerShell 5.1' }"
    } else {
        "if ($PSVersionTable.PSVersion -lt [version]'7.3') { throw 'Requires PowerShell 7.3+' }"
    };
    let script = format!(
        "$ErrorActionPreference = 'Stop'\n{guard}\n\
         $PSNativeCommandArgumentPassing = '{mode}'\n\
         Set-Alias curl Invoke-WebRequest -Option AllScope\n\
         {script}\n\
         if ($LASTEXITCODE -ne 0) {{ throw \"curl exited with $LASTEXITCODE\" }}\n"
    );
    let path = dir.join("command.ps1");
    // Windows PowerShell interprets BOM-less files using the ANSI code page.
    fs::write(
        &path,
        [b"\xef\xbb\xbf".as_slice(), script.as_bytes()].concat(),
    )
    .unwrap();
    let mut command = Command::new(executable);
    command
        .current_dir(dir)
        .env("CURL_HOME", dir)
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(path);
    command
}

fn expected_arguments(url: &str, body: &str) -> Vec<u8> {
    let args = [
        "--max-time",
        "5",
        "-X",
        "POST",
        url,
        "-H",
        "content-type: application/json",
        "-H",
        "accept: application/json, */*;q=0.5",
        "-d",
        body,
    ];
    let mut bytes = (args.len() as u32).to_le_bytes().to_vec();
    for arg in args {
        bytes.extend((arg.len() as u32).to_le_bytes());
        bytes.extend(arg.as_bytes());
    }
    bytes
}

fn capture_requests(listener: TcpListener, done: Arc<AtomicBool>) -> Vec<Vec<u8>> {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut bodies = Vec::new();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Windows can inherit the listener's nonblocking mode.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    assert_ne!(n, 0, "Truncated HTTP headers");
                    request.extend_from_slice(&buf[..n]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                    assert!(request.len() < 64 * 1024, "Oversized HTTP headers");
                };
                let headers = std::str::from_utf8(&request[..header_end]).unwrap();
                assert!(headers.starts_with("POST / HTTP/1.1\r\n"), "{headers}");
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .expect("Missing Content-Length");
                while request.len() - header_end < length {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    assert_ne!(n, 0, "Truncated request body");
                    request.extend_from_slice(&buf[..n]);
                }
                assert_eq!(request.len() - header_end, length);
                bodies.push(request[header_end..].to_vec());
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .unwrap();
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if done.load(Ordering::SeqCst) {
                    break;
                }
                assert!(Instant::now() < deadline, "Timed out waiting for curl");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(err) => panic!("HTTP accept failed: {err}"),
        }
    }
    bodies
}

#[test]
#[ignore = "requires Windows PowerShell 5.1, PowerShell 7.3+, rustc, and curl.exe"]
fn native_powershell_argument_and_request_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let probe_dir = dir.join("probe");
    fs::create_dir(&probe_dir).unwrap();
    success(
        Command::new("rustc")
            .args(["--edition=2021", "-C", "debuginfo=0"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/curl_argv.rs"))
            .arg("-o")
            .arg(probe_dir.join("curl.exe"))
            .output()
            .expect("rustc is required for the native argv probe"),
    );
    let original_path = std::env::var_os("PATH").unwrap();
    let probe_path = std::env::join_paths(
        std::iter::once(probe_dir).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();
    let records = dir.join("argv.bin");
    let curl_version = success(
        Command::new("curl.exe")
            .arg("--version")
            .output()
            .expect("curl.exe is required"),
    );
    println!("{}", String::from_utf8_lossy(&curl_version.stdout));
    save_evidence(
        "curl-version",
        serde_json::json!({
            "version": String::from_utf8_lossy(&curl_version.stdout),
        }),
    );

    // Each case is tested as an actual argv value AND an actual HTTP body.
    let cases = [
        ("issue-492", r#"{"name":"Carl"}"#),
        ("quote", r#"a"b"#),
        ("apostrophe", "can't"),
        ("mixed-quotes", r#"it's "quoted""#),
        ("backslashes", r"C:\one\two"),
        ("backslash-before-quote", r#"one\"two"#),
        ("two-backslashes-before-quote", r#"one\\"two"#),
        ("trailing-backslash", r"one\"),
        ("space-trailing-backslash", r"two words\"),
        ("space-two-trailing-backslashes", r"two words\\"),
        ("space-three-trailing-backslashes", r"two words\\\"),
        ("quote-space-trailing-backslash", r#"a"b c\"#),
        ("mixed-space-trailing-backslash", r#"it's "quoted" \"#),
        (
            "escaped-quote-space-trailing-backslash",
            r#"one\"two words\"#,
        ),
        (
            "escaped-quote-run-space-trailing-backslash",
            r#"one\\"two words\\"#,
        ),
        ("empty", ""),
        ("spaces", " two  words "),
        ("unicode", "你好 café 🦀"),
        (
            "shell-metacharacters",
            "$env:USERNAME; $(throw 'must remain data') | & `",
        ),
        (
            "json-combined",
            r#"{"quote":"\"","apostrophe":"can't","path":"C:\\two words\\","empty":"","unicode":"你好 🦀"}"#,
        ),
    ];
    for (executable, mode, shell) in [
        ("powershell.exe", "Legacy", Some("powershell-legacy")),
        ("pwsh.exe", "Legacy", Some("powershell-legacy")),
        ("pwsh.exe", "Windows", Some("powershell")),
        ("pwsh.exe", "Standard", Some("powershell")),
        ("powershell.exe", "Legacy", None),
        ("pwsh.exe", "Legacy", None),
    ] {
        let version = success(
            Command::new(executable)
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "$PSVersionTable.PSVersion.ToString()",
                ])
                .output()
                .unwrap_or_else(|err| {
                    panic!("{executable} required (matrix must not silently skip): {err}")
                }),
        );
        println!(
            "{executable} {} mode={mode} shell={shell:?}",
            String::from_utf8_lossy(&version.stdout).trim()
        );
        let matrix_id = format!("{executable}-{mode}-{}", shell.unwrap_or("default"));
        save_evidence(
            &format!("{matrix_id}-version"),
            serde_json::json!({
                "version": String::from_utf8_lossy(&version.stdout),
                "mode": mode,
                "shell": shell,
            }),
        );
        if mode == "Windows" || mode == "Standard" {
            // Negative control: the old quoting must reproduce issue #492 in
            // modern modes, with four extra literal backslashes in argv.
            let url = "http://127.0.0.1:1/";
            let script = translation(Some("powershell-legacy"), url, r#"{"name":"Carl"}"#, dir);
            let _ = fs::remove_file(&records);
            success(
                powershell(executable, mode, &script, dir)
                    .env("PATH", &probe_path)
                    .env("XH_CURL_ARGV_FILE", &records)
                    .output()
                    .unwrap(),
            );
            let actual = fs::read(&records).unwrap();
            let expected = expected_arguments(url, r#"{\"name\":\"Carl\"}"#);
            save_evidence(
                &format!("{matrix_id}-negative-control"),
                serde_json::json!({
                    "command": script, "expected": expected, "actual": actual,
                }),
            );
            assert_eq!(
                actual, expected,
                "Negative control: legacy quoting must overescape in {mode}"
            );
            println!("PASS negative control {executable}/{mode}: reproduced overescaping");
        }
        for (label, body) in cases {
            let url = "http://127.0.0.1:1/";
            let script = translation(shell, url, body, dir);
            let _ = fs::remove_file(&records);
            success(
                powershell(executable, mode, &script, dir)
                    .env("PATH", &probe_path)
                    .env("XH_CURL_ARGV_FILE", &records)
                    .output()
                    .unwrap(),
            );
            let actual = fs::read(&records).unwrap();
            let expected = expected_arguments(url, body);
            save_evidence(
                &format!("{matrix_id}-{label}-argv"),
                serde_json::json!({
                    "command": script, "expected": expected, "actual": actual,
                }),
            );
            assert_eq!(
                actual, expected,
                "argv: {executable}/{mode}/{shell:?}/{label}\n{script}"
            );

            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let script = translation(shell, &url, body, dir);
            let done = Arc::new(AtomicBool::new(false));
            let server_done = Arc::clone(&done);
            let server = std::thread::spawn(move || capture_requests(listener, server_done));
            let output = powershell(executable, mode, &script, dir).output().unwrap();
            done.store(true, Ordering::SeqCst);
            let bodies = server.join().unwrap();
            save_evidence(
                &format!("{matrix_id}-{label}-request"),
                serde_json::json!({
                    "command": script,
                    "expected": [body.as_bytes()], "actual": bodies,
                    "exit_success": output.status.success(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }),
            );
            success(output);
            assert_eq!(
                bodies,
                vec![body.as_bytes().to_vec()],
                "request: {executable}/{mode}/{shell:?}/{label}\n{script}"
            );
            println!(
                "PASS {executable}/{mode}/{shell:?}/{label}: exact argv, one request, {} body bytes",
                body.len()
            );
        }
    }
}
