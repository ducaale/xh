use std::io::{self, stderr, stdout, LineWriter, Stderr, Stdout, Write};

use crate::utils::test_pretend_term;

#[derive(Debug)]
pub enum Buffer {
    File(std::fs::File),
    Redirect(Stdout),
    Stdout(Stdout),
    Stderr(Stderr),
}

impl Buffer {
    pub fn new(download: bool, output: &Option<String>, is_stdout_tty: bool) -> io::Result<Self> {
        Ok(if download {
            Buffer::Stderr(stderr())
        } else if let Some(output) = output {
            let file = std::fs::File::create(&output)?;
            Buffer::File(file)
        } else if is_stdout_tty {
            Buffer::Stdout(stdout())
        } else {
            Buffer::Redirect(stdout())
        })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Buffer::Stdout(..) | Buffer::Stderr(..))
            || (matches!(self, Buffer::Redirect(..)) && test_pretend_term())
    }

    pub fn is_redirect(&self) -> bool {
        matches!(self, Buffer::Redirect(..))
    }

    pub fn print(&mut self, s: &str) -> io::Result<()> {
        write!(self.inner(), "{}", s)
    }

    fn inner(&mut self) -> &mut dyn Write {
        match self {
            Buffer::File(file) => file,
            Buffer::Redirect(stdout) | Buffer::Stdout(stdout) => stdout,
            Buffer::Stderr(stderr) => stderr,
        }
    }

    /// Use a [`Write`] handle that ensures no binary data is written to the
    /// terminal.
    ///
    /// This takes a closure in order to perform cleanup at the end.
    pub fn with_guard(
        &mut self,
        code: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.is_terminal() {
            // Wrapping a LineWriter around the guard means binary data
            // usually won't slip through even with very short writes. It also
            // means the supression message starts on a new line. HTTPie works
            // similarly.
            // If the LineWriter receives a very long line that doesn't fit in
            // its buffer it'll do a premature write. That's acceptable.
            // It's avoided by `HighlightWriter` because it breaks the
            // formatting, but there's no major concern here.
            let mut guard = LineWriter::new(BinaryGuard(self));
            code(&mut guard)?;
            // If the written text did not end in a newline we need to flush
            guard.flush()
        } else {
            code(self.inner())
        }
    }

    /// Get a [`Write`] handle to write data directly. This should be used
    /// after checking for binary content.
    pub fn unguarded(&mut self) -> &mut dyn Write {
        self.inner()
    }
}

struct BinaryGuard<'a>(&'a mut Buffer);

impl BinaryGuard<'_> {
    fn check_dirty(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.contains(&b'\0') {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Found binary data",
            ))
        } else {
            Ok(())
        }
    }
}

impl Write for BinaryGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_dirty(buf)?;
        self.0.inner().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.inner().flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.check_dirty(buf)?;
        self.0.inner().write_all(buf)
    }
}
