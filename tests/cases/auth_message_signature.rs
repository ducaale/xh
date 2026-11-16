use crate::{get_command as base_get_command, server};
use base64::engine::general_purpose::STANDARD;
use httpsig_hyper::HyperSigError;
use httpsig_hyper::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

const KEY_MATERIAL: &str = "secret-key-material";
const KEY_HEX: &str = "7365637265742d6b65792d6d6174657269616c";
const ED25519_KEY_HEX: &str = "0101010101010101010101010101010101010101010101010101010101010101";

#[cfg(feature = "online-tests")]
const ONLINE_KEY_ID: &str = "https://my.example.com/actor#main-key";
#[cfg(feature = "online-tests")]
const ONLINE_PUBLIC_KEY: &str = include_str!("../fixtures/keys/rsa_public_key.pem");

#[cfg(feature = "online-tests")]
fn httpsig_post(path: &str, body: serde_json::Value) -> serde_json::Value {
    reqwest::blocking::Client::new()
        .post(format!("https://httpsig.org{path}"))
        .json(&body)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap()
}

// Rebuild the signed HTTP message from httpbingo's echo so httpsig.org can
// verify exactly what xh sent. https://httpsig.org is the sandbox linked from
#[cfg(feature = "online-tests")]
fn signed_message(json: &serde_json::Value, tamper_digest: bool) -> (String, String, String) {
    use reqwest::Url;

    let url = Url::parse(json["url"].as_str().unwrap()).unwrap();
    let target = format!(
        "{}{}",
        url.path(),
        url.query()
            .map(|query| format!("?{query}"))
            .unwrap_or_default()
    );
    let host = json["headers"]["Host"][0].as_str().unwrap();
    let signature = json["headers"]["Signature"][0].as_str().unwrap();
    let signature_input = json["headers"]["Signature-Input"][0]
        .as_str()
        .unwrap()
        .to_string();
    let body = json["data"].as_str().unwrap();
    let digest = json["headers"]["Content-Digest"][0]
        .as_str()
        .unwrap_or_default();
    let digest_line = if digest.is_empty() {
        String::new()
    } else {
        let digest = if tamper_digest {
            let mut chars: Vec<char> = digest.chars().collect();
            let offset = "sha-256=:".len();
            chars[offset] = match chars[offset] {
                'A' => 'B',
                _ => 'A',
            };
            chars.into_iter().collect()
        } else {
            digest.to_string()
        };
        format!("Content-Digest: {digest}\n")
    };

    (
        format!(
            "{} {target} HTTP/1.1\nHost: {host}\nContent-Length: {}\n{digest_line}Signature: {signature}\nSignature-Input: {signature_input}\n\n{body}",
            json["method"].as_str().unwrap(),
            body.len()
        ),
        signature.split_once('=').unwrap().1.to_string(),
        signature_input,
    )
}

#[cfg(feature = "online-tests")]
fn verified_online(args: &[&str], covered: &[&str], tamper_digest: bool) -> bool {
    use serde_json::{Value, json};

    let mut cmd = base_get_command();
    cmd.arg(format!("--httpsig-keyid={ONLINE_KEY_ID}"))
        .arg("--httpsig-key=@tests/fixtures/keys/rsa_private_key_pkcs1.hex")
        .arg("--httpsig-algo=rsa-v1_5-sha256")
        .args(args);
    let assert = cmd.assert().success();
    let json: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    let (message, signature, signature_input) = signed_message(&json, tamper_digest);

    assert_eq!(json["headers"]["Signature"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["headers"]["Signature-Input"].as_array().unwrap().len(),
        1
    );
    for component in covered {
        assert!(signature_input.contains(&format!("\"{component}\"")));
    }
    assert!(signature_input.contains(";created="));
    assert!(signature_input.contains(";alg=\"rsa-v1_5-sha256\""));
    assert!(signature_input.contains(&format!(";keyid=\"{ONLINE_KEY_ID}\"")));

    let parsed = httpsig_post("/parse", json!({ "msg": message }));
    let created: i64 = signature_input
        .split("created=")
        .nth(1)
        .unwrap()
        .split([';', ')'])
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let base = httpsig_post(
        "/base",
        json!({
            "components": parsed["components"],
            "coveredComponents": covered.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
            "params": {
                "created": created,
                "alg": "rsa-v1_5-sha256",
                "keyid": ONLINE_KEY_ID,
            },
        }),
    );
    let result = httpsig_post(
        "/verify",
        json!({
            "httpMsg": message,
            "signatureInput": base["signatureInput"],
            "signatureParams": base["signatureParams"],
            "signingKeyType": "x509",
            "signingKeyX509": ONLINE_PUBLIC_KEY,
            "alg": "rsa-v1_5-sha256",
            "label": "sig1",
            "signature": signature,
        }),
    );
    result["signatureVerified"].as_bool().unwrap()
}

fn key_file(contents: impl AsRef<[u8]>) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    for byte in contents.as_ref() {
        write!(file, "{byte:02x}").unwrap();
    }
    file
}

fn reconstruct_absolute_uri<B>(req: &mut hyper::Request<B>) {
    // Reconstruct absolute URI for verification of @target-uri and @authority
    if let Some(host) = req.headers().get("host") {
        let host_str = host.to_str().unwrap();
        let uri_string = format!("http://{}{}", host_str, req.uri());
        *req.uri_mut() = uri_string.parse().unwrap();
    }
}

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
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();

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

    base_get_command()
        .arg(format!("--httpsig-keyid={}", key_id))
        .arg(format!("--httpsig-key={KEY_HEX}"))
        .arg("--httpsig-algo=hmac-sha256")
        .arg("--httpsig-headers=method path")
        .arg("--httpsig-headers=date:")
        .arg("get")
        .arg(server.base_url())
        .arg("date:Thu, 15 Jan 2026 12:00:00 GMT")
        .assert()
        .success();
}

#[test]
fn message_signature_ed25519_is_the_default() {
    let key_id = "ed25519-key";
    let server = server::http(move |mut req| async move {
        reconstruct_absolute_uri(&mut req);

        let secret_key = SecretKey::from_bytes(&AlgorithmName::Ed25519, &[1; 32]).unwrap();
        let public_key = secret_key.public_key();
        use httpsig_hyper::MessageSignatureReq;
        let result = req
            .verify_message_signature(&public_key, Some(key_id))
            .await;

        assert!(result.is_ok(), "Signature verification failed: {result:?}");
        assert!(
            req.headers()["Signature-Input"]
                .to_str()
                .unwrap()
                .contains("alg=\"ed25519\"")
        );
        hyper::Response::default()
    });

    base_get_command()
        .arg(format!("--httpsig-keyid={key_id}"))
        .arg(format!("--httpsig-key={ED25519_KEY_HEX}"))
        .arg("--httpsig-algo=ed25519")
        .arg("get")
        .arg(server.base_url())
        .assert()
        .success();
}

#[test]
fn message_signature_redirect_follow_re_signs_request() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";
    let key_file = key_file(key);

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            if req.uri().path() == "/redirect" {
                return hyper::Response::builder()
                    .status(302)
                    .header("Location", "/final")
                    .body(Default::default())
                    .unwrap();
            }

            assert_eq!(req.uri().path(), "/final");
            reconstruct_absolute_uri(&mut req);

            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();

            use httpsig_hyper::MessageSignatureReq;
            let result = req
                .verify_message_signature(&shared_key, Some(&key_id_inner))
                .await;
            assert!(
                result.is_ok(),
                "Signature verification failed on redirected request: {:?}",
                result.err()
            );

            hyper::Response::default()
        }
    });

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
        .arg("--follow")
        .arg("get")
        .arg(server.url("/redirect"))
        .assert()
        .success();
}

#[test]
fn message_signature_cross_origin_redirect_drops_signature() {
    let key_file = key_file(KEY_MATERIAL);
    let target = server::http(|req| async move {
        assert!(!req.headers().contains_key("Signature"));
        assert!(!req.headers().contains_key("Signature-Input"));
        hyper::Response::default()
    });
    let target_url = target.url("/final");
    let redirect = server::http(move |_req| {
        let target_url = target_url.clone();
        async move {
            hyper::Response::builder()
                .status(302)
                .header("Location", target_url)
                .body(Default::default())
                .unwrap()
        }
    });

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
        .arg("--follow")
        .arg("get")
        .arg(redirect.url("/redirect"))
        .assert()
        .success();
}

#[test]
fn message_signature_auth_defaults() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";
    let key_file = key_file(key);

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            reconstruct_absolute_uri(&mut req);

            assert_eq!(req.method(), "POST");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));

            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

            // Expect default components: @method, @authority, @path
            assert!(sig_input.contains("sig1="));
            assert!(sig_input.contains(r#""@method" "@authority" "@path""#));
            assert!(sig_input.contains(r#"keyid="my-key""#));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();
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

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
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
fn message_signature_auth_with_resolve_override() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";
    let key_file = key_file(key);

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            reconstruct_absolute_uri(&mut req);

            let host = req.headers()["host"].to_str().unwrap();
            assert!(
                host.starts_with("example.com"),
                "unexpected host header: {host}"
            );

            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();

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

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
        .arg(format!("--resolve=example.com:{}", server.host()))
        .arg("get")
        .arg(format!("http://example.com:{}/resolve", server.port()))
        .assert()
        .success();
}

#[test]
fn message_signature_auth_ipv6_authority() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";
    let key_file = key_file(key);

    let server = match server::http_v6(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            reconstruct_absolute_uri(&mut req);

            assert_eq!(req.method(), "GET");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();
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
    let mut cmd = base_get_command();
    cmd.arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
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
    let key_file = key_file(key);

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            reconstruct_absolute_uri(&mut req);

            assert_eq!(req.method(), "POST");
            assert!(req.headers().contains_key("Signature"));
            assert!(req.headers().contains_key("Signature-Input"));
            assert!(req.headers().contains_key("Content-Digest"));

            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();
            assert!(sig_input.contains(r#""@method" "@path" "content-digest""#));
            assert!(!sig_input.contains(r#""@authority""#)); // We overrode defaults

            let digest = req.headers()["Content-Digest"].to_str().unwrap();
            assert!(digest.starts_with("sha-256=:"));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();
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

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
        .arg("--httpsig-headers=method path content-digest:")
        .arg("-v")
        .arg("post")
        .arg(server.base_url())
        .arg("content-digest:sha-256=:O6iQfnolIydIjfOQ7VF8Rblt6tAzYAIZvcpxB9HT+Io=:")
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
    let key_file = key_file(key);

    let server = server::http(move |req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            let sig_input = req.headers()["Signature-Input"].to_str().unwrap();

            // Assertions for correctness:
            // 1. Label sig1 should be present
            assert!(sig_input.contains("sig1="));
            // 2. Derived RFC components are emitted with their canonical name.
            assert!(sig_input.contains("@method"));
            // 3. Set-Cookie should be present
            assert!(sig_input.contains(r#""set-cookie""#));
            // 4. keyid should be present
            assert!(sig_input.contains(r#"keyid="my-key""#));

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();
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

    base_get_command()
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
        .arg("--httpsig-headers=method set-cookie:")
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
fn message_signature_rejects_non_curl_component_syntax() {
    for components in [
        "@method authority",
        "method content-type",
        "\"x-struct\";sf",
        "\"x-dict\";key=\"a\"",
        "@query-param;name=\"id\"",
    ] {
        base_get_command()
            .arg("--offline")
            .arg("--httpsig-keyid=my-key")
            .arg(format!("--httpsig-key={KEY_HEX}"))
            .arg(format!("--httpsig-headers={components}"))
            .arg("get")
            .arg("https://example.com")
            .assert()
            .failure()
            .stderr(predicates::str::contains(
                "HTTP message signature component",
            ));
    }
}

#[test]
fn message_signature_components_require_key_pair() {
    // clap rejects --httpsig-headers without a key pair.
    base_get_command()
        .arg("--offline")
        .arg("--httpsig-headers=method")
        .arg("get")
        .arg("https://example.com")
        .assert()
        .failure()
        .stderr(predicates::str::contains("--httpsig-keyid <KEY_ID>"));
}

#[test]
fn message_signature_with_basic_auth() {
    let key = KEY_MATERIAL;
    let key_id = "my-key";
    let key_file = key_file(key);

    let server = server::http(move |mut req| {
        let key_inner = key.to_string();
        let key_id_inner = key_id.to_string();
        async move {
            reconstruct_absolute_uri(&mut req);

            assert!(req.headers().contains_key("Authorization"));
            assert!(req.headers().contains_key("Signature"));
            assert!(
                req.headers()["Authorization"]
                    .to_str()
                    .unwrap()
                    .starts_with("Basic ")
            );

            // Verify the signature
            use base64::Engine;
            let key_base64 = STANDARD.encode(&key_inner);
            let shared_key =
                SharedKey::from_base64(&AlgorithmName::HmacSha256, &key_base64).unwrap();
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

    base_get_command()
        .arg("--auth=user:pass")
        .arg("--auth-type=basic")
        .arg("--httpsig-keyid=my-key")
        .arg(format!("--httpsig-key=@{}", key_file.path().display()))
        .arg("--httpsig-algo=hmac-sha256")
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

#[cfg(feature = "online-tests")]
#[test]
fn post_signature_verified_online() {
    assert!(verified_online(
        &[
            "--print=b",
            "--httpsig-headers=method target-uri content-digest:",
            "POST",
            "https://httpbingo.org/anything",
            "x=1",
        ],
        &["@method", "@target-uri", "content-digest"],
        false,
    ));
}

#[cfg(feature = "online-tests")]
#[test]
fn get_signature_verified_without_content_digest_online() {
    assert!(verified_online(
        &[
            "--print=b",
            "--httpsig-headers=method target-uri content-digest:",
            "GET",
            "https://httpbingo.org/anything",
        ],
        &["@method", "@target-uri"],
        false,
    ));
}

#[cfg(feature = "online-tests")]
#[test]
fn tampered_content_digest_rejected_online() {
    assert!(!verified_online(
        &[
            "--print=b",
            "--httpsig-headers=method target-uri content-digest:",
            "POST",
            "https://httpbingo.org/anything",
            "x=1",
        ],
        &["@method", "@target-uri", "content-digest"],
        true,
    ));
}

#[test]
fn message_signature_missing_key_file_fails() {
    base_get_command()
        .arg("--httpsig-keyid=some-key")
        .arg("--httpsig-key=@non_existent_file.txt")
        .arg("--httpsig-algo=hmac-sha256")
        .arg("get")
        .arg("http://localhost:1")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "message-signature: Failed to read key file: non_existent_file.txt",
        ));
}
