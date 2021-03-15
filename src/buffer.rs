use std::{
    fmt,
    io::{self, stdout, Stdout},
};

use termcolor::{Ansi, ColorChoice, StandardStream, WriteColor};

use crate::{cli::Pretty, utils::test_pretend_term};

pub enum Buffer {
    File(Ansi<std::fs::File>),
    Redirect(Ansi<Stdout>),
    Stdout(StandardStream),
    Stderr(StandardStream),
}

impl Buffer {
    pub fn new(
        download: bool,
        output: &Option<String>,
        is_stdout_tty: bool,
        pretty: Option<Pretty>,
    ) -> io::Result<Self> {
        let color_choice = match pretty {
            None => ColorChoice::Auto,
            Some(pretty) if pretty.color() => ColorChoice::Always,
            _ => ColorChoice::Never,
        };
        Ok(if download {
            Buffer::Stderr(StandardStream::stderr(color_choice))
        } else if let Some(output) = output {
            let file = std::fs::File::create(&output)?;
            Buffer::File(Ansi::new(file))
        } else if is_stdout_tty {
            Buffer::Stdout(StandardStream::stdout(color_choice))
        } else {
            Buffer::Redirect(Ansi::new(stdout()))
        })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Buffer::Stdout(..) | Buffer::Stderr(..))
            || (matches!(self, Buffer::Redirect(..)) && test_pretend_term())
    }

    pub fn is_redirect(&self) -> bool {
        matches!(self, Buffer::Redirect(..))
    }

    pub fn print(&mut self, s: impl AsRef<[u8]>) -> io::Result<()> {
        self.inner_mut().write_all(s.as_ref())
    }

    fn inner(&self) -> &dyn WriteColor {
        match self {
            Buffer::File(file) => file,
            Buffer::Stdout(stream) | Buffer::Stderr(stream) => stream,
            Buffer::Redirect(stream) => stream,
        }
    }

    fn inner_mut(&mut self) -> &mut dyn WriteColor {
        match self {
            Buffer::File(file) => file,
            Buffer::Stdout(stream) | Buffer::Stderr(stream) => stream,
            Buffer::Redirect(stream) => stream,
        }
    }
}

impl io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner_mut().flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.inner_mut().write_all(buf)
    }
}

impl WriteColor for Buffer {
    fn supports_color(&self) -> bool {
        self.inner().supports_color()
    }

    fn set_color(&mut self, spec: &termcolor::ColorSpec) -> io::Result<()> {
        // We should only even attempt highlighting if coloring is supported
        debug_assert!(self.supports_color());
        // This one's called often, so avoid the overhead of dyn
        match self {
            Buffer::File(file) => file.set_color(spec),
            Buffer::Stdout(stream) | Buffer::Stderr(stream) => stream.set_color(spec),
            Buffer::Redirect(stream) => stream.set_color(spec),
        }
    }

    fn reset(&mut self) -> io::Result<()> {
        self.inner_mut().reset()
    }

    fn is_synchronous(&self) -> bool {
        self.inner().is_synchronous()
    }
}

// Cannot be derived because StandardStream doesn't implement it
impl fmt::Debug for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Buffer::File(..) => "File",
            Buffer::Stderr(..) => "Stderr",
            Buffer::Stdout(..) => "Stdout",
            Buffer::Redirect(..) => "Redirect",
        };
        write!(f, "{}(..)", text)
    }
}
