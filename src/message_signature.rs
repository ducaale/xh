use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use httpsig_hyper::prelude::{
    AlgorithmName, HttpSigResult, HttpSignatureParams, SecretKey, SharedKey, SigningKey,
    message_component::HttpMessageComponentId,
};
use hyper::http;
use reqwest::blocking::Request;
use reqwest::header::HeaderName;

use crate::cli::{HttpsigOptions, MessageSignatureComponent, MessageSignatureKey};

/// Apply the HTTP Message Signature options to a request.
///
/// Signing covers only headers that are actually present on the request, so
/// the signature matches exactly what will be sent.
pub fn sign_request(request: &mut Request, httpsig: &HttpsigOptions) -> Result<()> {
    // Match curl: a caller-provided Signature or Signature-Input means the
    // request is already signed and automatic signing is skipped entirely.
    if request.headers().contains_key("signature")
        || request.headers().contains_key("signature-input")
    {
        return Ok(());
    }

    let (key_id, key_source) = httpsig
        .key_pair()
        .context("message-signature: Missing key or key identifier")?;
    let key = load_key(key_source)?;
    let algorithm: AlgorithmName = httpsig.httpsig_algorithm.unwrap_or_default().into();
    let signing_key = build_signing_key(&key, key_id, &algorithm)?;

    let components = resolve_components(
        request,
        httpsig.httpsig_headers.as_ref().map(|c| c.0.as_slice()),
    );
    validate_header_components(request, &components)?;

    let mut signature_params = build_signature_params(&components)?;
    signature_params.set_alg(&algorithm);
    // curl always includes the user-provided key identifier in Signature-Input.
    signature_params.set_keyid(key_id);

    // httpsig-hyper signs http::Request, while the rest of xh uses reqwest::Request.
    // When the caller supplies Content-Digest, the body is covered through that
    // field; only request metadata needs to be copied into this temporary request.
    let mut http_request = http::Request::builder()
        .version(request.version())
        .method(request.method())
        .uri(request.url().as_str())
        .body(reqwest::Body::default())
        .context("message-signature: Failed to build temporary HTTP request")?;
    // Copy the header values being signed from the actual request so the
    // signature covers exactly what will be sent.
    for component in &components {
        if let MessageSignatureComponent::Header(name) = component {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .with_context(|| format!("message-signature: Invalid header component: {name}"))?;
            for value in request.headers().get_all(&header_name) {
                http_request
                    .headers_mut()
                    .append(header_name.clone(), value.clone());
            }
        }
    }

    use httpsig_hyper::MessageSignatureReqSync;
    http_request
        .set_message_signature_sync(&signature_params, &signing_key, Some("sig1"))
        .context("message-signature: Failed to set message signature")?;

    // Signing mutates the temporary request. Copy only the generated fields back so
    // the original reqwest body and all other request state remain intact.
    let signature = http_request
        .headers()
        .get("signature")
        .context("message-signature: Signature header missing after signing")?;
    let signature_input = http_request
        .headers()
        .get("signature-input")
        .context("message-signature: Signature-Input header missing after signing")?;

    request
        .headers_mut()
        .insert(HeaderName::from_static("signature"), signature.clone());
    request.headers_mut().insert(
        HeaderName::from_static("signature-input"),
        signature_input.clone(),
    );
    Ok(())
}

/// Resolve curl component names to the exact set covered by this request.
///
/// curl defaults to method, authority, and path, adding query only when the URL has
/// one. An explicitly requested Content-Digest is omitted when there is no body.
fn resolve_components(
    request: &Request,
    components: Option<&[MessageSignatureComponent]>,
) -> Vec<MessageSignatureComponent> {
    let Some(components) = components else {
        let mut defaults = vec![
            MessageSignatureComponent::Method,
            MessageSignatureComponent::Authority,
            MessageSignatureComponent::Path,
        ];
        if request.url().query().is_some() {
            defaults.push(MessageSignatureComponent::Query);
        }
        return defaults;
    };

    components
        .iter()
        .filter(|component| !component.is_header("content-digest") || request.body().is_some())
        .cloned()
        .collect()
}

/// Header components must resolve to a header that is actually present in the
/// request being signed.
fn validate_header_components(
    request: &Request,
    components: &[MessageSignatureComponent],
) -> Result<()> {
    for component in components {
        if let MessageSignatureComponent::Header(name) = component {
            if !request.headers().contains_key(name) {
                bail!("message-signature: Header `{name}:` not found in request");
            }
        }
    }
    Ok(())
}

/// Convert validated curl CLI components to RFC 9421 identifiers for httpsig-hyper.
fn build_signature_params(components: &[MessageSignatureComponent]) -> Result<HttpSignatureParams> {
    let component_ids = components
        .iter()
        .map(|component| {
            HttpMessageComponentId::try_from(component.rfc_name())
                .with_context(|| format!("message-signature: Invalid component: {component:?}"))
        })
        .collect::<Result<Vec<_>>>()?;
    HttpSignatureParams::try_new(&component_ids)
        .context("message-signature: Failed to create signature params")
}

/// Load the hex-encoded key material accepted by curl's `--httpsig-key` option.
fn load_key(source: &MessageSignatureKey) -> Result<Vec<u8>> {
    match source {
        MessageSignatureKey::Hex(key) => decode_hex_key(key)
            .context("message-signature: Key must be a non-empty hexadecimal value"),
        MessageSignatureKey::File(key_path) => {
            let path = crate::utils::expand_tilde(key_path);
            let key_material = std::fs::read_to_string(&path).with_context(|| {
                format!(
                    "message-signature: Failed to read key file: {}",
                    path.display()
                )
            })?;
            // Match curl's file2string() behavior: remove every line ending before
            // interpreting the complete file as one hexadecimal value.
            let hex: String = key_material
                .chars()
                .filter(|character| !matches!(character, '\r' | '\n'))
                .collect();
            decode_hex_key(&hex).ok_or_else(|| {
                anyhow!(
                    "message-signature: Key file must contain a hex-encoded key: {}",
                    path.display()
                )
            })
        }
    }
}

fn decode_hex_key(hex: &str) -> Option<Vec<u8>> {
    if hex.is_empty() || hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect()
}

fn build_hmac_signing_key(key_material: &[u8], key_id: &str) -> Result<MessageSigningKey> {
    // httpsig represents shared keys as base64 even though curl's CLI accepts hex.
    let encoded = STANDARD.encode(key_material);
    let shared_key = SharedKey::from_base64(&AlgorithmName::HmacSha256, &encoded)
        .map_err(|error| anyhow!("message-signature: Failed to create HMAC key: {error:?}"))?;
    Ok(MessageSigningKey::Shared(shared_key, key_id.to_string()))
}

fn build_signing_key(
    key_material: &[u8],
    key_id: &str,
    algorithm: &AlgorithmName,
) -> Result<MessageSigningKey> {
    if algorithm == &AlgorithmName::HmacSha256 {
        return build_hmac_signing_key(key_material, key_id);
    }

    if algorithm == &AlgorithmName::Ed25519 {
        // Match curl: ed25519 requires exactly 32 key bytes. httpsig 0.0.24
        // panics on any other length via copy_from_slice, so validate first.
        if key_material.len() != 32 {
            bail!(
                "message-signature: ed25519 requires a 32-byte key (got {})",
                key_material.len()
            );
        }
        // httpsig 0.0.24 panics while deriving a public key from this seed.
        if key_material.iter().all(|byte| *byte == 0) {
            bail!("message-signature: Ed25519 key must not be an all-zero seed");
        }
    }

    let secret = SecretKey::from_bytes(algorithm, key_material).with_context(|| {
        format!(
            "message-signature: Failed to parse private key bytes as {}",
            algorithm.as_str()
        )
    })?;
    Ok(MessageSigningKey::Secret(secret, key_id.to_string()))
}

/// Adapter that gives both asymmetric and shared keys one SigningKey interface.
enum MessageSigningKey {
    Secret(SecretKey, String),
    Shared(SharedKey, String),
}

impl SigningKey for MessageSigningKey {
    fn sign(&self, data: &[u8]) -> HttpSigResult<Vec<u8>> {
        match self {
            MessageSigningKey::Secret(inner, _) => inner.sign(data),
            MessageSigningKey::Shared(inner, _) => inner.sign(data),
        }
    }

    fn key_id(&self) -> String {
        match self {
            MessageSigningKey::Secret(_, id) => id.clone(),
            MessageSigningKey::Shared(_, id) => id.clone(),
        }
    }

    fn alg(&self) -> AlgorithmName {
        match self {
            MessageSigningKey::Secret(inner, _) => inner.alg(),
            MessageSigningKey::Shared(inner, _) => inner.alg(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::blocking::Client;
    use reqwest::header::HeaderValue;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn key_file(contents: impl AsRef<[u8]>) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(contents.as_ref()).unwrap();
        file
    }

    #[test]
    fn key_file_ignores_crlf_like_curl() {
        let file = key_file("7365\r\n6372\n6574");
        let key = load_key(&MessageSignatureKey::File(
            file.path().as_os_str().to_owned(),
        ))
        .unwrap();
        assert_eq!(key, b"secret");
    }

    #[test]
    fn key_file_rejects_non_hex_and_pem() {
        for contents in [
            "secret\nignored",
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n",
        ] {
            let file = key_file(contents);
            let error = load_key(&MessageSignatureKey::File(
                file.path().as_os_str().to_owned(),
            ))
            .unwrap_err();
            assert!(error.to_string().contains("hex-encoded key"));
        }
    }

    #[test]
    fn key_sources_decode_hex() {
        let direct = load_key(&MessageSignatureKey::Hex("736563726574".to_string())).unwrap();
        assert_eq!(direct, b"secret");

        let file = key_file("736563726574\n");
        let from_file = load_key(&MessageSignatureKey::File(
            file.path().as_os_str().to_owned(),
        ))
        .unwrap();
        assert_eq!(from_file, b"secret");
    }

    #[test]
    fn all_zero_ed25519_seed_is_rejected_without_panicking() {
        let error = build_signing_key(&[0; 32], "test-key", &AlgorithmName::Ed25519)
            .err()
            .unwrap();
        assert!(error.to_string().contains("all-zero seed"));
    }

    #[test]
    fn non_32_byte_ed25519_key_is_rejected_without_panicking() {
        for bytes in [&b""[..], &b"\x01\x02"[..], &[0x01; 16][..], &[0x01; 64][..]] {
            let error = build_signing_key(bytes, "test-key", &AlgorithmName::Ed25519)
                .err()
                .unwrap();
            assert!(
                error.to_string().contains("32-byte key"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn default_components_include_query_only_when_present() {
        let request = Client::new().get("http://a.com").build().unwrap();
        assert_eq!(
            resolve_components(&request, None),
            vec![
                MessageSignatureComponent::Method,
                MessageSignatureComponent::Authority,
                MessageSignatureComponent::Path,
            ]
        );

        let request = Client::new()
            .get("https://example.com/?id=1")
            .build()
            .unwrap();
        assert_eq!(
            resolve_components(&request, None),
            vec![
                MessageSignatureComponent::Method,
                MessageSignatureComponent::Authority,
                MessageSignatureComponent::Path,
                MessageSignatureComponent::Query,
            ]
        );
    }

    #[test]
    fn preprovided_signature_headers_skip_automatic_signing() {
        let options = HttpsigOptions {
            httpsig_key_id: Some("key".to_string()),
            httpsig_key: Some(MessageSignatureKey::Hex("736563726574".to_string())),
            httpsig_algorithm: Some(crate::cli::MessageSignatureAlgorithm::HmacSha256),
            httpsig_headers: None,
        };
        for name in ["signature", "signature-input"] {
            let mut request = Client::new()
                .get("http://example.com")
                .header(name, HeaderValue::from_static("provided"))
                .build()
                .unwrap();
            let before = request.headers().clone();

            sign_request(&mut request, &options).unwrap();

            assert_eq!(request.headers(), &before);
        }
    }

    #[test]
    fn header_components_sign_only_headers_present_in_request() {
        let options = HttpsigOptions {
            httpsig_key_id: Some("key".to_string()),
            httpsig_key: Some(MessageSignatureKey::Hex("736563726574".to_string())),
            httpsig_algorithm: Some(crate::cli::MessageSignatureAlgorithm::HmacSha256),
            httpsig_headers: Some(crate::cli::MessageSignatureComponents(vec![
                MessageSignatureComponent::Header("user-agent".to_string()),
            ])),
        };

        // A generated default present on the request is signable.
        let mut request = Client::new()
            .get("http://example.com")
            .header("user-agent", "xh/0.0.0")
            .build()
            .unwrap();
        sign_request(&mut request, &options).unwrap();
        assert!(request.headers().contains_key("signature"));

        // An absent header cannot be signed.
        let mut request = Client::new().get("http://example.com").build().unwrap();
        let error = sign_request(&mut request, &options).unwrap_err();
        assert!(error.to_string().contains("Header `user-agent:` not found"));
    }
}
