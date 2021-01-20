use std::io::Write;

use atty::Stream;

pub enum Buffer {
    File(std::fs::File),
    Redirect,
    Stdout,
    Stderr
}

impl Buffer {
    pub fn new(download: bool, output: &Option<String>) -> std::io::Result<Self> {
        let buffer = if download {
            Buffer::Stderr
        } else if let Some(output) = output {
            let file = std::fs::File::open(&output)?;
            Buffer::File(file)
        } else if atty::is(Stream::Stdout) {
            Buffer::Stdout
        } else {
            Buffer::Redirect
        };
        Ok(buffer)
    }

    pub fn write(&mut self, s: &str) {
        match self {
            Buffer::Redirect => print!("{}", &s),
            Buffer::Stdout => print!("{}", &s),
            Buffer::Stderr => eprint!("{}", &s),
            Buffer::File(ref mut f) => write!(f, "{}", &s).unwrap()
        }
    }

    pub fn write_bytes(&mut self, s: &[u8]) {
        match self {
            Buffer::Redirect => std::io::stdout().write(&s).unwrap(),
            Buffer::Stdout => std::io::stdout().write(&s).unwrap(),
            Buffer::Stderr => std::io::stderr().write(&s).unwrap(),
            Buffer::File(ref mut f) => f.write(&s).unwrap()
        };
    }
}
