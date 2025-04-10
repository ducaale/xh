use percent_encoding::percent_decode;

/// Parse filename from Content-Disposition header
/// Prioritizes filename* parameter if present, otherwise uses filename parameter
pub fn parse_filename_from_content_disposition(content_disposition: &str) -> Option<String> {
    let parts: Vec<&str> = content_disposition
        .split(';')
        .map(|part| part.trim())
        .collect();

    // First try to find filename* parameter
    for part in parts.iter() {
        if part.starts_with("filename*=") {
            if let Some(filename) = parse_encoded_filename(part) {
                return Some(filename);
            }
        }
    }

    // If filename* is not found or parsing failed, try regular filename parameter
    for part in parts {
        if part.starts_with("filename=") {
            return parse_regular_filename(part);
        }
    }

    None
}

/// Parse regular filename parameter
/// Handles both quoted and unquoted filenames
fn parse_regular_filename(part: &str) -> Option<String> {
    let filename = part.trim_start_matches("filename=");
    // Remove quotes if present
    //
    // Content-Disposition: attachment; filename="file with \"quotes\".txt"  // This won't occur
    // Content-Disposition: attachment; filename*=UTF-8''file%20with%20quotes.txt  // This is the actual practice
    //
    // We don't need to handle escaped characters in Content-Disposition header parsing because:
    //
    // It's not a standard practice
    // It rarely occurs in real-world scenarios
    // When filenames contain special characters, they should use the filename* parameter
    let filename = if filename.starts_with('"') && filename.ends_with('"') {
        &filename[1..(filename.len() - 1)]
    } else {
        filename
    };

    if filename.is_empty() {
        return None;
    }

    Some(filename.to_string())
}

/// Parse RFC 5987 encoded filename (filename*)
/// Format: charset'language'encoded-value
fn parse_encoded_filename(part: &str) -> Option<String> {
    // Remove "filename*=" prefix
    let content = part.trim_start_matches("filename*=");

    // According to RFC 5987, format should be: charset'language'encoded-value
    let parts: Vec<&str> = content.splitn(3, '\'').collect();
    if parts.len() != 3 {
        return None;
    }

    let encoded_filename = parts[2];

    // Decode using percent-encoding
    let decoded = percent_decode(encoded_filename.as_bytes())
        .decode_utf8()
        .ok()?;

    Some(decoded.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_filename() {
        let header = r#"attachment; filename="example.pdf""#;
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("example.pdf".to_string())
        );
    }

    #[test]
    fn test_filename_without_quotes() {
        let header = "attachment; filename=example.pdf";
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("example.pdf".to_string())
        );
    }

    #[test]
    fn test_encoded_filename() {
        // UTF-8 encoded Chinese filename "测试.pdf"
        let header = "attachment; filename*=UTF-8''%E6%B5%8B%E8%AF%95.pdf";
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("测试.pdf".to_string())
        );
    }

    #[test]
    fn test_both_filenames() {
        // When both filename and filename* are present, filename* should be preferred
        let header =
            r#"attachment; filename="fallback.pdf"; filename*=UTF-8''%E6%B5%8B%E8%AF%95.pdf"#;
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("测试.pdf".to_string())
        );
    }

    #[test]
    fn test_no_filename() {
        let header = "attachment";
        assert_eq!(parse_filename_from_content_disposition(header), None);
    }
}
