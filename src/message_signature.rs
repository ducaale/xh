use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use httpsig::prelude::{
    message_component::{
        DerivedComponentName, HttpMessageComponent, HttpMessageComponentId,
        HttpMessageComponentName, HttpMessageComponentParam,
    },
    AlgorithmName, HttpSigResult, HttpSignatureBase, HttpSignatureParams, SecretKey, SharedKey,
    SigningKey,
};
use reqwest::blocking::{Body as ReqwestBody, Request};
use reqwest::header::{HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use url::Url;

use crate::utils::HeaderValueExt;

pub fn sign_request(
    request: &mut Request,
    key_id: &str,
    key_material: &str,
    components: Option<&str>,
) -> Result<()> {
    let key = if let Some(path) = key_material.strip_prefix('@') {
        std::fs::read(crate::utils::expand_tilde(path))?
    } else {
        // Unlike some HTTPie plugins that force Base64 encoding for the secret key part
        // of the --auth string, xh treats the raw string as the key material by default.
        // This provides a more direct CLI experience, consistent with how xh handles
        // standard passwords in `-a user:password`.
        key_material.as_bytes().to_vec()
    };

    let components_vec = components.map(|cp| {
        cp.split(',')
            .map(|s| {
                let component = s.trim();
                if let Some(idx) = component.find(';') {
                    let (name, params) = component.split_at(idx);
                    format!("{}{}", name.to_lowercase(), params)
                } else {
                    component.to_lowercase()
                }
            })
            .collect::<Vec<String>>()
    });

    let components = resolve_components(request, components_vec.as_deref());
    ensure_content_digest(request, &components)?;

    let mut signature_params = build_signature_params(&components)?;

    let algorithm = determine_alg_from_key(&key)?;
    signature_params.set_alg(&algorithm);

    let algorithm_for_key = determine_alg_from_key(&key)?;
    let signing_key = build_signing_key(&key, algorithm_for_key, key_id)?;

    // Ensure keyid is included in Signature-Input
    signature_params.set_keyid(key_id);

    let query_params = QueryParams::from_url(request.url());

    let component_lines = build_component_lines(request, &signature_params, &query_params)?;
    let signature_base = HttpSignatureBase::try_new(&component_lines, &signature_params)
        .context("message-signature: Failed to build signature base")?;

    // We use "sig1" as the label for now
    let label = "sig1";

    let headers = signature_base
        .build_signature_headers(&signing_key, Some(label))
        .context("message-signature: Failed to build signature headers")?;

    request.headers_mut().insert(
        HeaderName::from_static("signature"),
        HeaderValue::from_str(&headers.signature_header_value())?,
    );
    request.headers_mut().insert(
        HeaderName::from_static("signature-input"),
        HeaderValue::from_str(&headers.signature_input_header_value())?,
    );

    Ok(())
}

/// Resolves and expands message components for signature coverage.
///
/// This function handles:
/// - Default components: If no components are specified, uses @method, @authority, @target-uri
/// - @query-params expansion: Expands into individual @query-param components for each parameter
/// - content-digest: Only includes if the request has a body
///
/// Note: @query-params is not a standard RFC 9421 component, but is commonly used as a
/// convenience shorthand to sign all query parameters without listing them individually.
fn resolve_components(request: &Request, components: Option<&[String]>) -> Vec<String> {
    let mut resolved = Vec::new();
    let source = if let Some(c) = components {
        c
    } else {
        // RFC 9421 recommended minimal set for request signing
        &[
            "@method".to_string(),
            "@authority".to_string(),
            "@target-uri".to_string(),
        ] as &[String]
    };

    for component in source {
        if component == "@query-params" {
            // According to some conventions (and this implementation), "@query-params"
            // acts as a wildcard that expands into individual "@query-param" components
            // for every parameter present in the request's query string.
            //
            // RFC 9421 does not define "@query-params" as a standard derived component,
            // but many implementations use it to simplify signing all query parameters
            // without listing them explicitly.
            if let Some(query) = request.url().query() {
                for (name, _) in form_urlencoded::parse(query.as_bytes()) {
                    resolved.push(format!("@query-param;name=\"{}\"", name));
                }
            }
        } else if component == "content-digest" {
            if request.body().is_some() {
                resolved.push(component.clone());
            }
        } else {
            resolved.push(component.clone());
        }
    }
    resolved
}

/// Ensures the Content-Digest header is present if it's a covered component.
///
/// According to RFC 9530, the Content-Digest header uses the format:
/// `sha-256=:<base64-encoded-hash>:`
///
/// This function:
/// 1. Checks if "content-digest" is in the covered components
/// 2. If yes and the header is missing, computes SHA-256 of the request body
/// 3. Adds the Content-Digest header in the RFC 9530 format
fn ensure_content_digest(request: &mut Request, components: &[String]) -> Result<()> {
    if components
        .iter()
        .any(|c| c.eq_ignore_ascii_case("content-digest"))
        && !request.headers().contains_key("content-digest")
        && request.body().is_some()
    {
        let bytes = buffer_request_body(request)?;
        let digest = Sha256::digest(&bytes);
        // RFC 9530 format: algorithm=:base64-hash:
        let value = format!("sha-256=:{}:", STANDARD.encode(digest));
        request.headers_mut().insert(
            HeaderName::from_static("content-digest"),
            HeaderValue::from_str(&value)?,
        );
    }
    Ok(())
}

fn build_signature_params(components: &[String]) -> Result<HttpSignatureParams> {
    let mut component_ids = Vec::new();
    for c in components {
        let normalized = normalize_component_id(c);
        let id = HttpMessageComponentId::try_from(normalized.as_str())
            .with_context(|| format!("message-signature: Invalid component: {}", c))?;
        component_ids.push(id);
    }
    HttpSignatureParams::try_new(&component_ids)
        .context("message-signature: Failed to create signature params")
}

/// Normalizes component identifiers for RFC 9421 compliance.
///
/// According to RFC 9421, derived components (starting with @) that have parameters
/// must be quoted. For example:
/// - `@query-param;name="foo"` -> `"@query-param";name="foo"`
/// - `@method` -> `@method` (no parameters, no quotes needed)
/// - `content-type` -> `content-type` (not a derived component)
///
/// This normalization is required for proper signature base construction.
fn normalize_component_id(component: &str) -> String {
    if let Some(idx) = component.find(';') {
        let (name, params) = component.split_at(idx);
        if name.starts_with('@') && !name.starts_with('"') {
            // Derived component with parameters must be quoted
            return format!("\"{}\"{}", name, params);
        }
    }
    component.to_string()
}

fn buffer_request_body(request: &mut Request) -> Result<Vec<u8>> {
    if let Some(body) = request.body_mut() {
        let bytes = body
            .buffer()
            .context("message-signature: Failed to buffer request body for Content-Digest")?
            .to_vec();
        *body = ReqwestBody::from(bytes.clone());
        Ok(bytes)
    } else {
        Ok(Vec::new())
    }
}

/// Determines the signing algorithm from the key material.
///
/// Algorithm detection logic:
/// 1. If the key is in PEM format (contains "-----BEGIN"), parse it and use its algorithm
///    (e.g., RSA-PSS-SHA512, ECDSA-P256-SHA256, Ed25519)
/// 2. Otherwise, assume it's a raw symmetric key and default to HMAC-SHA256
///
/// This allows xh to support both asymmetric (PEM) and symmetric (raw) keys seamlessly.
fn determine_alg_from_key(key_material: &[u8]) -> Result<AlgorithmName> {
    if let Ok(pem) = std::str::from_utf8(key_material) {
        if pem.contains("-----BEGIN") {
            if let Ok(secret) = SecretKey::from_pem(pem) {
                return Ok(secret.alg());
            }
        }
    }
    // Default to HMAC-SHA256 for raw key material
    Ok(AlgorithmName::HmacSha256)
}

fn build_signing_key(
    key_material: &[u8],
    algorithm: AlgorithmName,
    key_id: &str,
) -> Result<MessageSigningKey> {
    let text = std::str::from_utf8(key_material).ok();

    if let Some(pem) = text {
        if pem.contains("-----BEGIN") {
            let secret = SecretKey::from_pem(pem)
                .context("message-signature: Failed to parse private key PEM for signing")?;
            return Ok(MessageSigningKey::Secret(secret, key_id.to_string()));
        }
    }

    match algorithm {
        AlgorithmName::HmacSha256 => {
            let encoded = STANDARD.encode(key_material);
            let shared_key = SharedKey::from_base64(&encoded)
                .map_err(|e| anyhow!("message-signature: Failed to create HMAC key: {:?}", e))?;
            Ok(MessageSigningKey::Shared(shared_key, key_id.to_string()))
        }
        _ => {
            let secret = SecretKey::from_bytes(algorithm, key_material)
                .context("message-signature: Failed to parse private key bytes")?;
            Ok(MessageSigningKey::Secret(secret, key_id.to_string()))
        }
    }
}

fn build_component_lines(
    request: &Request,
    params: &HttpSignatureParams,
    query_params: &QueryParams,
) -> Result<Vec<HttpMessageComponent>> {
    let mut components = Vec::new();
    for component_id in &params.covered_components {
        let values = gather_component_values(request, component_id, query_params)?;

        // TODO: RFC 9421 Section 2.1.1 states that 'set-cookie' MUST NOT be combined
        // into a single field value and should be treated as separate values (multiple lines).
        // Currently, they are combined with a comma because httpsig's HttpSignatureBase
        // requires the number of component lines to match the number of covered component IDs.
        // Supporting this correctly would require duplicating the component ID in
        // Signature-Input or using a library version that handles multi-value sets automatically.
        components.push(
            HttpMessageComponent::try_from((component_id, values.as_slice()))
                .context("message-signature: Failed to build HTTP message component")?,
        );
    }
    Ok(components)
}

fn gather_component_values(
    request: &Request,
    component_id: &HttpMessageComponentId,
    query_params: &QueryParams,
) -> Result<Vec<String>> {
    match &component_id.name {
        HttpMessageComponentName::Derived(derived) => {
            gather_derived_component_values(request, derived, component_id, query_params)
        }
        HttpMessageComponentName::HttpField(field) => gather_http_field_values(request, field),
    }
}

fn gather_http_field_values(request: &Request, field: &str) -> Result<Vec<String>> {
    let name = field.to_ascii_lowercase();
    let header_name = HeaderName::from_bytes(name.as_bytes()).with_context(|| {
        format!("message-signature: Invalid header name in Signature-Input: {field}")
    })?;
    let values = request.headers().get_all(&header_name);
    if values.iter().next().is_none() {
        bail!("message-signature: Signature-Input refers to header '{field}', but the request does not include it");
    }
    let mut collected = Vec::new();
    for value in values.iter() {
        // According to RFC 9421 Section 2.1, the value of an HTTP field component
        // is the field value with leading and trailing whitespace removed.
        let s = header_value_to_string(value)?;
        collected.push(s.trim().to_string());
    }
    Ok(collected)
}

fn gather_derived_component_values(
    request: &Request,
    derived: &DerivedComponentName,
    component_id: &HttpMessageComponentId,
    query_params: &QueryParams,
) -> Result<Vec<String>> {
    let url = request.url();
    match derived {
        DerivedComponentName::Method => Ok(vec![request.method().as_str().to_string()]),
        DerivedComponentName::TargetUri => Ok(vec![url.as_str().to_string()]),
        DerivedComponentName::Authority => Ok(vec![compute_authority(url)]),
        DerivedComponentName::Scheme => Ok(vec![url.scheme().to_ascii_lowercase()]),
        DerivedComponentName::RequestTarget => Ok(vec![compute_request_target(request)]),
        DerivedComponentName::Path => Ok(vec![compute_path(url)]),
        DerivedComponentName::Query => Ok(vec![compute_query(url)]),
        DerivedComponentName::QueryParam => gather_query_param_values(query_params, component_id),
        DerivedComponentName::SignatureParams => {
            bail!(
                "message-signature: @signature-params must not be included as a covered component"
            );
        }
        DerivedComponentName::Status => {
            bail!("message-signature: @status derived component is only valid in responses");
        }
    }
}

fn compute_authority(url: &Url) -> String {
    // According to RFC 9421 Section 2.2.3, the "@authority" derived component
    // consists of the host and, if present and non-default, the port number.
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if let Some(port) = url.port() {
        if Some(port) != default_port_for_scheme(url.scheme()) {
            return format!("{host}:{port}");
        }
    }
    host
}

fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

fn compute_request_target(request: &Request) -> String {
    if request.method() == reqwest::Method::CONNECT {
        let url = request.url();
        return compute_authority(url);
    }
    let url = request.url();
    let mut target = url.path().to_string();
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    target
}

/// Computes the @path derived component value according to RFC 9421 Section 2.2.6.
///
/// The @path component is the absolute path of the request target with no query component
/// and no trailing question mark. According to RFC 9421:
/// - An empty path string is normalized as a single slash ("/") character
/// - Path components are represented before decoding any percent-encoded octets
///
/// For example:
/// - URL "https://example.com/path?query" -> "/path"
/// - URL "https://example.com" -> "/"
/// - URL "https://example.com/" -> "/"
fn compute_path(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() {
        // RFC 9421 Section 2.2.6: empty path is normalized as "/"
        "/".to_string()
    } else {
        path.to_string()
    }
}

/// Computes the @query derived component value according to RFC 9421 Section 2.2.7.
///
/// The @query component is the entire normalized query string including the leading "?"
/// character. According to RFC 9421 Section 2.2.7:
/// - When a query is present: the value is "?" + query string
/// - When the query is absent: the value is the leading "?" character alone
///
/// This behavior is CORRECT per RFC 9421. Do NOT return an empty string when query is absent,
/// as that would violate the specification and cause signature verification failures with
/// RFC-compliant verifiers.
///
/// For example:
/// - URL "https://example.com/path?param=value" -> "?param=value"
/// - URL "https://example.com/path" -> "?"
fn compute_query(url: &Url) -> String {
    match url.query() {
        Some(q) => format!("?{q}"),
        // RFC 9421 Section 2.2.7: "If the query string is absent from the request message,
        // the component value is the leading ? character alone"
        None => "?".to_string(),
    }
}

fn gather_query_param_values(
    query_params: &QueryParams,
    component_id: &HttpMessageComponentId,
) -> Result<Vec<String>> {
    let name = component_id
        .params
        .0
        .iter()
        .find_map(|param| match param {
            HttpMessageComponentParam::Name(name) => Some(name.as_str()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("message-signature: @query-param requires a name parameter"))?;

    let values = query_params
        .params
        .get(name)
        .ok_or_else(|| anyhow!("message-signature: Query parameter '{name}' is not present"))?;

    Ok(values.clone())
}

struct QueryParams {
    params: HashMap<String, Vec<String>>,
}

impl QueryParams {
    fn from_url(url: &Url) -> Self {
        let mut params: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(query) = url.query() {
            // According to RFC 9421 Section 2.2.8.1, the value of the "@query-param"
            // component is the percent-decoded value of the parameter.
            // form_urlencoded::parse handles the percent-decoding and preserves order.
            for (name, value) in form_urlencoded::parse(query.as_bytes()) {
                params
                    .entry(name.into_owned())
                    .or_default()
                    .push(value.into_owned());
            }
        }
        QueryParams { params }
    }
}

fn header_value_to_string(value: &HeaderValue) -> Result<String> {
    match value.to_ascii_or_latin1() {
        Ok(s) => Ok(s.to_string()),
        Err(bad) => Ok(bad.latin1()),
    }
}

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

    #[test]
    fn test_content_digest_generation() {
        let mut req = Client::new()
            .post("http://example.com")
            .body("Hello, World!")
            .build()
            .unwrap();

        let components = vec!["content-digest".to_string()];
        ensure_content_digest(&mut req, &components).unwrap();

        let digest_header = req.headers().get("content-digest").unwrap();
        let digest_str = digest_header.to_str().unwrap();

        // SHA-256 of "Hello, World!" is dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f
        // Base64: 3/1gIbsr1bCvZ2KQgJ7DpTGR3YHH9wpLKGiKNiGCmG8=
        // Header format: sha-256=:...:
        assert_eq!(
            digest_str,
            "sha-256=:3/1gIbsr1bCvZ2KQgJ7DpTGR3YHH9wpLKGiKNiGCmG8=:"
        );
    }

    #[test]
    fn test_component_gathering_derived() {
        let req = Client::new()
            .post("https://example.com/foo?bar=baz")
            .header("Date", "Tue, 20 Apr 2021 02:07:55 GMT")
            .build()
            .unwrap();

        let query_params = QueryParams::from_url(req.url());

        // @method
        let id_method = HttpMessageComponentId::try_from("@method").unwrap();
        let values = gather_component_values(&req, &id_method, &query_params).unwrap();
        assert_eq!(values, vec!["POST"]);

        // @target-uri
        let id_uri = HttpMessageComponentId::try_from("@target-uri").unwrap();
        let values = gather_component_values(&req, &id_uri, &query_params).unwrap();
        assert_eq!(values, vec!["https://example.com/foo?bar=baz"]);

        // @authority
        let id_auth = HttpMessageComponentId::try_from("@authority").unwrap();
        let values = gather_component_values(&req, &id_auth, &query_params).unwrap();
        assert_eq!(values, vec!["example.com"]); // default port 443 omitted

        // @path
        let id_path = HttpMessageComponentId::try_from("@path").unwrap();
        let values = gather_component_values(&req, &id_path, &query_params).unwrap();
        assert_eq!(values, vec!["/foo"]);

        // @query
        let id_query = HttpMessageComponentId::try_from("@query").unwrap();
        let values = gather_component_values(&req, &id_query, &query_params).unwrap();
        assert_eq!(values, vec!["?bar=baz"]);
    }

    #[test]
    fn test_sign_request_with_query_param() {
        let mut req = Client::new()
            .get("https://example.com/?param=value")
            .build()
            .unwrap();

        let key_id = "test-key";
        let key_material = "secret";

        // Use the plural @query-params which expands automatically
        let components = "@method,@query-params";

        // This will internally call resolve_components -> try_from("@query-param;name=\"param\"")
        // If this succeeds, then the logic is correct.
        sign_request(&mut req, key_id, key_material, Some(components)).unwrap();

        let sig_input = req.headers()["signature-input"].to_str().unwrap();
        assert!(sig_input.contains("sig1="));
        // Check that the expanded component is present
        assert!(sig_input.contains("\"@query-param\";name=\"param\""));
    }

    #[test]
    fn test_sign_request_hmac() {
        let mut req = Client::new()
            .post("https://example.com/foo")
            .body("data")
            .build()
            .unwrap();

        let key_id = "test-key";
        let key_material = "secret"; // HMAC key

        // Explicitly include content-digest
        let components = "@method,@authority,content-digest";
        sign_request(&mut req, key_id, key_material, Some(components)).unwrap();

        assert!(req.headers().contains_key("signature"));
        assert!(req.headers().contains_key("signature-input"));
        assert!(req.headers().contains_key("content-digest"));

        let sig_input = req.headers()["signature-input"].to_str().unwrap();
        assert!(sig_input.contains("sig1="));
        assert!(sig_input.contains("keyid=\"test-key\""));
        assert!(sig_input.contains("content-digest"));
        assert!(sig_input.contains("alg=\"hmac-sha256\""));
    }

    #[test]
    fn test_bs_parameter_unsupported() {
        let mut req = Client::new()
            .get("https://example.com")
            .header("x-data", "hello")
            .build()
            .unwrap();

        let key_id = "test-key";
        let key_material = "secret";

        // Attempt to sign with the ;bs parameter which is currently unsupported by the underlying library
        let components = "\"x-data\";bs";
        let result = sign_request(&mut req, key_id, key_material, Some(components));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err().unwrap());
        // The underlying library httpsig currently returns "Not yet implemented: `bs` is not supported yet"
        assert!(err_msg.contains("not supported"));
    }

    #[test]
    fn test_sf_parameter_success() {
        let mut req = Client::new()
            .get("https://example.com")
            .header("x-struct", "a=1, b=2")
            .build()
            .unwrap();

        // ;sf is implemented in the underlying library
        let components = "\"x-struct\";sf";
        let result = sign_request(&mut req, "key1", "secret", Some(components));
        assert!(result.is_ok(), "sf parameter should be supported");
    }

    #[test]
    fn test_key_parameter_success() {
        let mut req = Client::new()
            .get("https://example.com")
            .header("x-dict", "a=1, b=2")
            .build()
            .unwrap();

        // ;key is implemented in the underlying library
        let components = "\"x-dict\";key=\"a\"";
        let result = sign_request(&mut req, "key1", "secret", Some(components));
        assert!(result.is_ok(), "key parameter should be supported");
    }

    #[test]
    fn test_tr_parameter_unsupported() {
        let mut req = Client::new()
            .get("https://example.com")
            .header("x-field", "value")
            .build()
            .unwrap();

        // ;tr is explicitly NOT implemented in the underlying library
        let components = "\"x-field\";tr";
        let result = sign_request(&mut req, "key1", "secret", Some(components));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(err_msg.contains("tr") && err_msg.contains("supported"));
    }

    #[test]
    fn test_name_parameter_error_on_field() {
        let mut req = Client::new()
            .get("https://example.com")
            .header("x-field", "value")
            .build()
            .unwrap();

        // ;name is only for @query-param, using it on a regular field should error
        let components = "\"x-field\";name=\"id\"";
        let result = sign_request(&mut req, "key1", "secret", Some(components));

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err().unwrap());
        // It could be either a validation error or a parsing error depending on the library version
        assert!(err_msg.contains("name"));
    }

    #[test]
    fn test_resolve_components_defaults() {
        let req = Client::new().get("http://a.com").build().unwrap();

        let defaults = resolve_components(&req, None);
        assert_eq!(defaults, vec!["@method", "@authority", "@target-uri"]);
    }

    #[test]
    fn test_normalize_component_id() {
        // Should wrap @ components with parameters in quotes
        assert_eq!(
            normalize_component_id("@query-param;name=\"a\""),
            "\"@query-param\";name=\"a\""
        );
        // Should not wrap if already wrapped
        assert_eq!(
            normalize_component_id("\"@query-param\";name=\"a\""),
            "\"@query-param\";name=\"a\""
        );
        // Should not wrap regular headers
        assert_eq!(normalize_component_id("content-type"), "content-type");
        // Should not wrap @ components without parameters
        assert_eq!(normalize_component_id("@method"), "@method");
    }

    #[test]
    fn test_set_cookie_gathering() {
        let req = Client::new()
            .get("https://example.com")
            .header("set-cookie", "a=1")
            .header("set-cookie", "b=2")
            .build()
            .unwrap();

        let values = gather_http_field_values(&req, "set-cookie").unwrap();
        // We expect individual values, not a joined string
        assert_eq!(values, vec!["a=1", "b=2"]);
    }

    #[test]
    fn test_header_trimming() {
        let req = Client::new()
            .get("https://example.com")
            .header("x-test", "  value  ")
            .build()
            .unwrap();

        let values = gather_http_field_values(&req, "x-test").unwrap();
        assert_eq!(values, vec!["value"]);
    }
}
