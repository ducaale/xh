use std::io::{stderr, stdout, Write};

use crate::printer::BINARY_SUPPRESSOR;

#[derive(Debug)]
pub struct Buffer {
    pub kind: BufferKind,
    dirty: bool,
}

#[derive(Debug)]
pub enum BufferKind {
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
        let kind = if download {
            BufferKind::Stderr
        } else if let Some(output) = output {
            let file = std::fs::File::create(&output)?;
            BufferKind::File(file)
        } else if is_stdout_tty {
            BufferKind::Stdout
        } else {
            BufferKind::Redirect
        };
        Ok(Buffer { kind, dirty: false })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.kind, BufferKind::Stdout | BufferKind::Stderr)
    }

    pub fn print(&mut self, s: &str) -> std::io::Result<()> {
        write!(self, "{}", s)
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.dirty {
            return Ok(buf.len());
        }
        if self.is_terminal() && buf.contains(&b'\0') {
            self.print("\x1b[0m")?;
            self.print(BINARY_SUPPRESSOR)?;
            self.dirty = true;
            return Ok(buf.len());
        }
        match &mut self.kind {
            BufferKind::File(file) => file.write(buf),
            BufferKind::Redirect | BufferKind::Stdout => stdout().write(buf),
            BufferKind::Stderr => stderr().write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.kind {
            BufferKind::File(file) => file.flush(),
            BufferKind::Redirect | BufferKind::Stdout => stdout().flush(),
            BufferKind::Stderr => stderr().flush(),
        }
    }
}
