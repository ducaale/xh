use std::process::ExitCode;

pub(crate) fn exit_code(err: &anyhow::Error) -> ExitCode {
    if let Some(err) = err.downcast_ref::<reqwest::Error>() {
        if err.is_timeout() {
            return ExitCode::from(2);
        }
    }

    if err
        .root_cause()
        .to_string()
        .starts_with("Too many redirects")
    {
        return ExitCode::from(6);
    }

    ExitCode::FAILURE
}
