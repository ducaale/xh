use std::{
    fmt,
    io::{self, stdout, LineWriter, Stdout, Write},
    path::Path,
};

use termcolor::{Ansi, ColorChoice, StandardStream, WriteColor};

use crate::{
    cli::Pretty,
    utils::{test_default_color, test_pretend_term},
};

pub enum Buffer {
    // These are all line-buffered (File explicitly, the others implicitly)
    // Line buffering gives unsurprising behavior but can still be a lot
    // faster than no buffering, especially with lots of small writes from
    // coloring
    File(Ansi<LineWriter<std::fs::File>>),
    Redirect(Ansi<Stdout>),
    Stdout(StandardStream),
    Stderr(StandardStream),
}

impl Buffer {
    pub fn new(
        download: bool,
        output: Option<&Path>,
        is_stdout_tty: bool,
        pretty: Option<Pretty>,
    ) -> io::Result<Self> {
        let color_choice = match pretty {
            None if test_default_color() => ColorChoice::AlwaysAnsi,
            None => ColorChoice::Auto,
            Some(pretty) if pretty.color() => ColorChoice::Always,
            _ => ColorChoice::Never,
        };
        Ok(if download {
            Buffer::Stderr(StandardStream::stderr(color_choice))
        } else if let Some(output) = output {
            let file = std::fs::File::create(&output)?;
            Buffer::File(Ansi::new(LineWriter::new(file)))
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

    #[inline]
    pub fn print(&mut self, s: impl AsRef<[u8]>) -> io::Result<()> {
        self.write_all(s.as_ref())
    }

    pub fn guess_pretty(&self) -> Pretty {
        if test_default_color() {
            Pretty::all
        } else if test_pretend_term() {
            Pretty::format
        } else if self.is_terminal() {
            // supports_color() considers $TERM, $NO_COLOR, etc
            // This lets us do the right thing with the progress bar
            if self.supports_color() {
                Pretty::all
            } else {
                Pretty::format
            }
        } else {
            Pretty::none
        }
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

impl Write for Buffer {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Buffer::File(file) => file.write(buf),
            Buffer::Stdout(stream) | Buffer::Stderr(stream) => stream.write(buf),
            Buffer::Redirect(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner_mut().flush()
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            Buffer::File(file) => file.write_all(buf),
            Buffer::Stdout(stream) | Buffer::Stderr(stream) => stream.write_all(buf),
            Buffer::Redirect(stream) => stream.write_all(buf),
        }
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
