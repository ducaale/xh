use std::io::{stderr, stdout, Write};

#[derive(Debug)]
pub enum Buffer {
    File(std::fs::File),
    Redirect,
    Stdout,
    Stderr,
}

impl Buffer {
    pub fn new(
        download: bool,
        output: &Option<String>,
        is_stdout_tty: bool,
    ) -> std::io::Result<Self> {
        let buffer = if download {
            Buffer::Stderr
        } else if let Some(output) = output {
            let file = std::fs::File::create(&output)?;
            Buffer::File(file)
        } else if is_stdout_tty {
            Buffer::Stdout
        } else {
            Buffer::Redirect
        };
        Ok(buffer)
    }

    pub fn print(&mut self, s: &str) -> std::io::Result<()> {
        write!(self, "{}", s)
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Buffer::File(file) => file.write(buf),
            Buffer::Redirect | Buffer::Stdout => stdout().write(buf),
            Buffer::Stderr => stderr().write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Buffer::File(file) => file.flush(),
            Buffer::Redirect | Buffer::Stdout => stdout().flush(),
            Buffer::Stderr => stderr().flush(),
        }
    }
}
