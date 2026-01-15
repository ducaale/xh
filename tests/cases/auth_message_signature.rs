use crate::{get_command, server};

#[test]
fn message_signature_auth_defaults() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert!(req.headers().contains_key("Signature"));
        assert!(req.headers().contains_key("Signature-Input"));

        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

        // Expect default components: @method, @authority, @target-uri
        assert!(sig_input.contains("sig1="));
        assert!(sig_input.contains(r#""@method" "@authority" "@target-uri""#));
        assert!(sig_input.contains(r#"keyid="my-key""#));

        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("-v")
        .arg("post")
        .arg(server.base_url())
        .arg("foo=bar")
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature: sig1="))
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_with_custom_components_and_digest() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        assert_eq!(req.method(), "POST");
        assert!(req.headers().contains_key("Signature"));
        assert!(req.headers().contains_key("Signature-Input"));
        assert!(req.headers().contains_key("Content-Digest"));

        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
        assert!(sig_input.contains(r#""@method" "@target-uri" "content-digest""#));
        assert!(!sig_input.contains(r#""@authority""#)); // We overrode defaults

        let digest = req.headers()["Content-Digest"].to_str().unwrap();
        assert!(digest.starts_with("sha-256=:"));

        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=@method,@target-uri,content-digest")
        .arg("-v")
        .arg("post")
        .arg(server.base_url())
        .arg("foo=bar")
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature: sig1="))
        .stdout(predicates::str::contains("Signature-Input: sig1="))
        .stdout(predicates::str::contains("Content-Digest: sha-256="));
}

#[test]
fn message_signature_auth_with_multiple_set_cookie() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

        // Assertions for correctness:
        // 1. Label sig1 should be present
        assert!(sig_input.contains("sig1="));
        // 2. normalize_component_id: @method should NOT be quoted if no params
        assert!(sig_input.contains("@method"));
        // 3. Set-Cookie should be present
        assert!(sig_input.contains(r#""set-cookie""#));
        // 4. keyid should be present
        assert!(sig_input.contains(r#"keyid="my-key""#));

        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=@method,set-cookie")
        .arg("-v")
        .arg("get")
        .arg(server.base_url())
        .arg("set-cookie:a=1")
        .arg("set-cookie:b=2")
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature: sig1="))
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_normalization_assertion() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

        // Assert normalize_component_id: "@query-param" should be quoted because it has params
        // Even if input as @query-param;name="id", it should be normalized to "@query-param";name="id"
        assert!(sig_input.contains(r#""@query-param";name="id""#));

        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=@method,@query-param;name=\"id\"")
        .arg("-v")
        .arg("get")
        .arg(format!("{}/?id=123", server.base_url()))
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_sf_parameter() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
        assert!(sig_input.contains(r#""x-struct";sf"#));
        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=\"x-struct\";sf")
        .arg("-v")
        .arg("get")
        .arg(server.base_url())
        .arg("x-struct:a=1, b=2")
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_key_parameter() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
        assert!(sig_input.contains(r#""x-dict";key="a""#));
        hyper::Response::default()
    });

    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=\"x-dict\";key=\"a\"")
        .arg("-v")
        .arg("get")
        .arg(server.base_url())
        .arg("x-dict:a=1, b=2")
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_unsupported_parameters() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let url = "http://localhost:1";

    // Test ;bs (Byte Sequence) - currently unsupported by httpsig
    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=\"x-data\";bs")
        .arg("get")
        .arg(url)
        .arg("x-data:hello")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not supported"));

    // Test ;tr (Trailers) - currently unsupported by httpsig
    get_command()
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("--unstable-m-sig-comp=\"x-field\";tr")
        .arg("get")
        .arg(url)
        .arg("x-field:value")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not supported"));
}

#[test]
fn message_signature_with_basic_auth() {
    let key = "IyMjc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3Nzc3M=";
    let server = server::http(|req| async move {
        assert!(req.headers().contains_key("Authorization"));
        assert!(req.headers().contains_key("Signature"));
        assert!(req.headers()["Authorization"]
            .to_str()
            .unwrap()
            .starts_with("Basic "));
        hyper::Response::default()
    });

    get_command()
        .arg("--auth=user:pass")
        .arg("--auth-type=basic")
        .arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("-v")
        .arg("get")
        .arg(server.base_url())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Authorization: Basic dXNlcjpwYXNz",
        ))
        .stdout(predicates::str::contains("Signature: sig1="));
}
