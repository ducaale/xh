use percent_encoding::percent_decode_str;

/// Parse filename from Content-Disposition header
/// Prioritizes filename* parameter if present, otherwise uses filename parameter
pub fn parse_filename_from_content_disposition(content_disposition: &str) -> Option<String> {
    let parts: Vec<&str> = content_disposition
        .split(';')
        .map(|part| part.trim())
        .collect();

    // First try to find filename* parameter
    for part in parts.iter() {
        if let Some(value) = part.strip_prefix("filename*=") {
            if let Some(filename) = parse_encoded_filename(value) {
                return Some(filename);
            }
        }
    }

    // If filename* is not found or parsing failed, try regular filename parameter
    for part in parts {
        if let Some(value) = part.strip_prefix("filename=") {
            return parse_regular_filename(value);
        }
    }

    None
}

/// Parse regular filename parameter
/// Handles both quoted and unquoted filenames
fn parse_regular_filename(filename: &str) -> Option<String> {
    // Content-Disposition: attachment; filename="file with \"quotes\".txt"  // This won't occur
    // Content-Disposition: attachment; filename*=UTF-8''file%20with%20quotes.txt  // This is the actual practice
    //
    // We don't need to handle escaped characters in Content-Disposition header parsing because:
    //
    // It's not a standard practice
    // It rarely occurs in real-world scenarios
    // When filenames contain special characters, they should use the filename* parameter

    // Remove quotes if present
    let filename = if filename.starts_with('"') && filename.ends_with('"') && filename.len() >= 2 {
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
fn parse_encoded_filename(content: &str) -> Option<String> {
    // Remove "filename*=" prefix

    // According to RFC 5987, format should be: charset'language'encoded-value
    let parts: Vec<&str> = content.splitn(3, '\'').collect();
    if parts.len() != 3 {
        return None;
    }
    let charset = parts[0];
    let encoded_filename = parts[2];

    // Percent-decode the encoded filename into bytes.
    let decoded_bytes = percent_decode_str(encoded_filename).collect::<Vec<u8>>();

    if charset.eq_ignore_ascii_case("UTF-8") {
        if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
            return Some(decoded_str);
        }
    } else if charset.eq_ignore_ascii_case("ISO-8859-1") {
        // Use the encoding_rs crate to decode ISO-8859-1 bytes.
        let decoded: String = decoded_bytes.iter().map(|&b| b as char).collect();
        return Some(decoded);
    } else {
        // Unknown charset. As a fallback, try interpreting as UTF-8.
        if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
            return Some(decoded_str);
        }
    }

    None
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
    fn test_both_filenames_with_bad_format() {
        // When both filename and filename* are present, filename* with bad format, filename should be used
        let header = r#"attachment; filename="fallback.pdf"; filename*=UTF-8'bad_format.pdf"#;
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("fallback.pdf".to_string())
        );
    }

    #[test]
    fn test_no_filename() {
        let header = "attachment";
        assert_eq!(parse_filename_from_content_disposition(header), None);
    }

    #[test]
    fn test_iso_8859_1() {
        let header = "attachment;filename*=iso-8859-1'en'%A3%20rates";
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("£ rates".to_string())
        );
    }

    #[test]
    fn test_bad_encoding_fallback_to_utf8() {
        let header = "attachment;filename*=UTF-16''%E6%B5%8B%E8%AF%95.pdf";
        assert_eq!(
            parse_filename_from_content_disposition(header),
            Some("测试.pdf".to_string())
        );
    }
}
