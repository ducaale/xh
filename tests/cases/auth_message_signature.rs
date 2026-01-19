use crate::{get_command, server};
use base64::engine::general_purpose::STANDARD;
use httpsig_hyper::prelude::*;
use httpsig_hyper::HyperSigError;

const KEY_MATERIAL: &str = "secret-key-material";

#[test]
fn message_signature_verification_on_server() {
    let key_id = "test-key";
    let key_material = KEY_MATERIAL;

    let server = server::http(move |req| {
        let key_id_inner = key_id.to_string();
        let key_material_inner = key_material.to_string();
        async move {
            // 1. Prepare the verification key (HMAC SHA256)
            use base64::Engine;
            let key_base64 = STANDARD.encode(key_material_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();

            // 2. Verify the request using extension trait provided by httpsig-hyper
            use httpsig_hyper::MessageSignatureReq;
            let result: Result<String, HyperSigError> = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;

            if result.is_ok() {
                hyper::Response::new(Default::default())
            } else {
                hyper::Response::builder()
                    .status(401)
                    .body(Default::default())
                    .unwrap()
            }
        }
    });

    get_command()
        .arg(format!("--unstable-m-sig-id={}", key_id))
        .arg(format!("--unstable-m-sig-key={}", key_material))
        .arg("--unstable-m-sig-comp=@method,@path,date")
        .arg("get")
        .arg(server.base_url())
        .arg("date: Thu, 15 Jan 2026 12:00:00 GMT")
        .assert()
        .success();
}

#[test]
fn message_signature_auth_defaults() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            // Reconstruct absolute URI for verification of @target-uri and @authority
            if let Some(host) = req.headers().get("host") {
                let host_str = host.to_str().unwrap();
                let uri_string = format!("http://{}{}", host_str, req.uri());
                *req.uri_mut() = uri_string.parse().unwrap();
            }

            assert_eq!(req.method(), "POST");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));

            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

            // Expect default components: @method, @authority, @target-uri
            assert!(sig_input.contains("sig1="));
            assert!(sig_input.contains(r#""@method" "@authority" "@target-uri""#));
            assert!(sig_input.contains(r#"keyid="my-key""#));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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
fn message_signature_auth_ipv6_authority() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = match server::http_v6(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            // Reconstruct absolute URI for verification of @target-uri and @authority
            if let Some(host) = req.headers().get("host") {
                let host_str = host.to_str().unwrap();
                let uri_string = format!("http://{}{}", host_str, req.uri());
                *req.uri_mut() = uri_string.parse().unwrap();
            }

            assert_eq!(req.method(), "GET");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
    }) {
        Some(server) => server,
        None => {
            eprintln!("IPv6 not available; skipping test");
            return;
        }
    };

    let host = server.host();
    let url = if host.contains(':') {
        format!("http://[{host}]:{}", server.port())
    } else {
        format!("http://{host}:{}", server.port())
    };
    let mut cmd = get_command();
    cmd.arg("--unstable-m-sig-id=my-key")
        .arg(format!("--unstable-m-sig-key={}", key))
        .arg("-v")
        .arg("get")
        .arg(url)
        .assert()
        .success()
        .stdout(predicates::str::contains("Signature: sig1="))
        .stdout(predicates::str::contains("Signature-Input: sig1="));
}

#[test]
fn message_signature_auth_with_custom_components_and_digest() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            // Reconstruct absolute URI for verification of @target-uri and @authority
            if let Some(host) = req.headers().get("host") {
                let host_str = host.to_str().unwrap();
                let uri_string = format!("http://{}{}", host_str, req.uri());
                *req.uri_mut() = uri_string.parse().unwrap();
            }

            assert_eq!(req.method(), "POST");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));
            assert!(req.headers().contains_key("Content-Digest"));

            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
            assert!(sig_input.contains(r#""@method" "@target-uri" "content-digest""#));
            assert!(!sig_input.contains(r#""@authority""#)); // We overrode defaults

            let digest = req.headers()["Content-Digest"].to_str().unwrap();
            assert!(digest.starts_with("sha-256=:"));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
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

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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
fn message_signature_auth_sf_parameter() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
            assert!(sig_input.contains(r#""x-struct";sf"#));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
            assert!(sig_input.contains(r#""x-dict";key="a""#));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;

            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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
    let key = KEY_MATERIAL;
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
    let key = KEY_MATERIAL;
    let key_id = "my-key";

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            // Reconstruct absolute URI for verification of @target-uri and @authority
            if let Some(host) = req.headers().get("host") {
                let host_str = host.to_str().unwrap();
                let uri_string = format!("http://{}{}", host_str, req.uri());
                *req.uri_mut() = uri_string.parse().unwrap();
            }

            assert!(req.headers().contains_key("Authorization"));
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers()["Authorization"]
                .to_str()
                .unwrap()
                .starts_with("Basic "));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key = SharedKey::from_base64(&key_base64).unwrap();
            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
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

#[test]
fn message_signature_auth_normalization_assertion() {
    let key = KEY_MATERIAL;

    let server = server::http(move |req| {
        async move {
            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

            // Assert normalize_component_id: "@query-param" should be quoted because it has params
            // Even if input as @query-param;name="id", it should be normalized to "@query-param";name="id"
            assert!(sig_input.contains(r#""@query-param";name="id""#));

            hyper::Response::default()
        }
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
fn message_signature_auth_ed25519_pem() {
    // Generated Ed25519 private key in PEM format
    let key_pem = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIJthSCf1pnwSYvdXIrXHikXUix0dmvLEm2JwWF+87xKG
-----END PRIVATE KEY-----"#;
    let key_id = "ed25519-key";

    let server = server::http(move |mut req| {
        let key_pem_inner = key_pem.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            // Reconstruct absolute URI for verification of @target-uri and @authority
            if let Some(host) = req.headers().get("host") {
                let host_str = host.to_str().unwrap();
                let uri_string = format!("http://{}{}", host_str, req.uri());
                *req.uri_mut() = uri_string.parse().unwrap();
            }

            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
            assert!(sig_input.contains("alg=\"ed25519\""));
            assert!(sig_input.contains(r#"keyid="ed25519-key""#));

            // Verify the signature using the public key
            let secret_key = SecretKey::from_pem(&key_pem_inner).unwrap();
            let public_key = secret_key.public_key();

            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&public_key, Some(&key_id_inner))
                .await;

            assert!(
                result.is_ok(),
                "Signature verification failed: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
    });

    get_command()
        .arg("--unstable-m-sig-id=ed25519-key")
        .arg(format!("--unstable-m-sig-key={}", key_pem))
        .arg("get")
        .arg(server.base_url())
        .assert()
        .success();
}
