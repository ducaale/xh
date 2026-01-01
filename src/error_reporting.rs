use std::process::ExitCode;

pub(crate) fn additional_messages(err: &anyhow::Error, native_tls: bool) -> Vec<String> {
    let mut msgs = Vec::new();

    #[cfg(feature = "rustls")]
    msgs.extend(format_rustls_error(err));

    if native_tls && err.root_cause().to_string() == "invalid minimum TLS version for backend" {
        msgs.push("Try running without the --native-tls flag.".into());
    }

    msgs
}

/// Format certificate expired/not valid yet messages. By default these print
/// human-unfriendly Unix timestamps.
///
/// Other rustls error messages (e.g. wrong host) are readable enough.
///
/// Note this only works on platforms where rustls-platform-verifier uses webpki for verification.
#[cfg(feature = "rustls")]
fn format_rustls_error(err: &anyhow::Error) -> Option<String> {
    use humantime::format_duration;
    use rustls::pki_types::UnixTime;
    use rustls::CertificateError;
    use time::OffsetDateTime;

    // Multiple layers of io::Error for some reason?
    // This may be fragile
    let err = err.root_cause().downcast_ref::<std::io::Error>()?;
    let err = err.get_ref()?.downcast_ref::<std::io::Error>()?;
    let err = err.get_ref()?.downcast_ref::<rustls::Error>()?;
    let rustls::Error::InvalidCertificate(err) = err else {
        return None;
    };

    fn conv_time(unix_time: &UnixTime) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(unix_time.as_secs() as i64).ok()
    }

    match err {
        CertificateError::ExpiredContext { time, not_after } => {
            let time = conv_time(time)?;
            let not_after = conv_time(not_after)?;
            let diff = format_duration((time - not_after).try_into().ok()?);
            Some(format!(
                "Certificate not valid after {not_after} ({diff} ago).",
            ))
        }
        CertificateError::NotValidYetContext { time, not_before } => {
            let time = conv_time(time)?;
            let not_before = conv_time(not_before)?;
            let diff = format_duration((not_before - time).try_into().ok()?);
            Some(format!(
                "Certificate not valid before {not_before} ({diff} from now).",
            ))
        }
        _ => None,
    }
}

pub(crate) fn exit_code(err: &anyhow::Error) -> ExitCode {
    if let Some(err) = err.downcast_ref::<reqwest::Error>() {
        if err.is_timeout() {
            return ExitCode::from(2);
        }
    }

    if err
        .downcast_ref::<crate::redirect::TooManyRedirects>()
        .is_some()
    {
        return ExitCode::from(6);
    }

    ExitCode::FAILURE
}
