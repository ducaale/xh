use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::ops::Deref;
use std::str::FromStr;

/// A String that doesn't show up in Debug representations.
///
/// This is important for logging, where we maybe want to avoid outputting
/// sensitive data.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl FromStr for SecretString {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl Deref for SecretString {
    type Target = String;

    fn deref(&self) -> &String {
        &self.0
    }
}

impl Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Uncomment this to see the string anyway:
        // self.0.fmt(f);
        // If that turns out to be frequently necessary we could
        // make this configurable at runtime, e.g. by flipping an
        // AtomicBool depending on an environment variable.
        f.write_str("(redacted)")
    }
}

impl From<SecretString> for OsString {
    fn from(string: SecretString) -> OsString {
        string.0.into()
    }
}
