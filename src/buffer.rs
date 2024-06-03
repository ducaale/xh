//! The [`Buffer`] type is responsible for writing the program output, be it
//! to a terminal or a pipe or a file. It supports colored output using
//! `termcolor`'s `WriteColor` trait.
//!
//! It's always buffered, so `.flush()` should be called whenever no new
//! output is immediately available. That's inconvenient, but improves
//! throughput.
//!
//! We want slightly different implementations depending on the platform and
//! the runtime conditions. Ansi<BufWriter> is fast, so we go through that
//! when possible, but on Windows we often need a BufferedStandardStream
//! instead to use the terminal APIs.
//!
//! Most of this code is boilerplate.

use std::{
    env::var_os,
    io::{self, Write},
    path::Path,
};

use crate::{
    cli::Pretty,
    utils::{test_default_color, test_pretend_term},
};

pub use imp::Buffer;

#[cfg(not(windows))]
mod imp {
    use std::io::{BufWriter, Write};

    use termcolor::{Ansi, WriteColor};

    pub struct Buffer {
        inner: Ansi<BufWriter<Inner>>,
        terminal: bool,
        redirect: bool,
    }

    enum Inner {
        File(std::fs::File),
        Stdout(std::io::Stdout),
        Stderr(std::io::Stderr),
    }

    impl Buffer {
        pub fn stdout() -> Self {
            Self {
                inner: Ansi::new(BufWriter::new(Inner::Stdout(std::io::stdout()))),
                terminal: true,
                redirect: false,
            }
        }

        pub fn stderr() -> Self {
            Self {
                inner: Ansi::new(BufWriter::new(Inner::Stderr(std::io::stderr()))),
                terminal: true,
                redirect: false,
            }
        }

        pub fn redirect() -> Self {
            Self {
                inner: Ansi::new(BufWriter::new(Inner::Stdout(std::io::stdout()))),
                terminal: crate::test_pretend_term(),
                redirect: true,
            }
        }

        pub fn file(file: std::fs::File) -> Self {
            Self {
                inner: Ansi::new(BufWriter::new(Inner::File(file))),
                terminal: false,
                redirect: false,
            }
        }

        pub fn is_terminal(&self) -> bool {
            self.terminal
        }

        pub fn is_redirect(&self) -> bool {
            self.redirect
        }

        #[cfg(test)]
        pub fn is_stdout(&self) -> bool {
            matches!(self.inner.get_ref().get_ref(), Inner::Stdout(_))
        }

        #[cfg(test)]
        pub fn is_stderr(&self) -> bool {
            matches!(self.inner.get_ref().get_ref(), Inner::Stderr(_))
        }

        #[cfg(test)]
        pub fn is_file(&self) -> bool {
            matches!(self.inner.get_ref().get_ref(), Inner::File(_))
        }
    }

    impl Write for Inner {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self {
                Inner::File(w) => w.write(buf),
                Inner::Stdout(w) => w.write(buf),
                Inner::Stderr(w) => w.write(buf),
            }
        }

        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            match self {
                Inner::File(w) => w.write_all(buf),
                Inner::Stdout(w) => w.write_all(buf),
                Inner::Stderr(w) => w.write_all(buf),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Inner::File(w) => w.flush(),
                Inner::Stdout(w) => w.flush(),
                Inner::Stderr(w) => w.flush(),
            }
        }
    }

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.inner.write(buf)
        }

        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            // get_mut() to directly write into the BufWriter is significantly faster
            // https://github.com/BurntSushi/termcolor/pull/56
            self.inner.get_mut().write_all(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    impl WriteColor for Buffer {
        fn supports_color(&self) -> bool {
            true
        }

        fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
            self.inner.set_color(spec)
        }

        fn reset(&mut self) -> std::io::Result<()> {
            self.inner.reset()
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io::{BufWriter, Write};

    use termcolor::{Ansi, BufferedStandardStream, ColorChoice, WriteColor};

    use crate::utils::test_default_color;

    pub enum Buffer {
        // Only escape codes make sense when the output isn't going directly
        // to a terminal, so we use Ansi for some cases.
        File(Ansi<BufWriter<std::fs::File>>),
        Redirect(Ansi<BufWriter<std::io::Stdout>>),
        Stdout(BufferedStandardStream),
        Stderr(BufferedStandardStream),
    }

    impl Buffer {
        pub fn stdout() -> Self {
            Buffer::Stdout(BufferedStandardStream::stdout(if test_default_color() {
                ColorChoice::AlwaysAnsi
            } else {
                ColorChoice::Always
            }))
        }

        pub fn stderr() -> Self {
            Buffer::Stderr(BufferedStandardStream::stderr(if test_default_color() {
                ColorChoice::AlwaysAnsi
            } else {
                ColorChoice::Always
            }))
        }

        pub fn redirect() -> Self {
            Buffer::Redirect(Ansi::new(BufWriter::new(std::io::stdout())))
        }

        pub fn file(file: std::fs::File) -> Self {
            Buffer::File(Ansi::new(BufWriter::new(file)))
        }

        pub fn is_terminal(&self) -> bool {
            matches!(self, Buffer::Stdout(_) | Buffer::Stderr(_))
        }

        pub fn is_redirect(&self) -> bool {
            matches!(self, Buffer::Redirect(_))
        }

        #[cfg(test)]
        pub fn is_stdout(&self) -> bool {
            matches!(self, Buffer::Stdout(_))
        }

        #[cfg(test)]
        pub fn is_stderr(&self) -> bool {
            matches!(self, Buffer::Stderr(_))
        }

        #[cfg(test)]
        pub fn is_file(&self) -> bool {
            matches!(self, Buffer::File(_))
        }
    }

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self {
                Buffer::File(w) => w.write(buf),
                Buffer::Redirect(w) => w.write(buf),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.write(buf),
            }
        }

        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            match self {
                Buffer::File(w) => w.get_mut().write_all(buf),
                Buffer::Redirect(w) => w.get_mut().write_all(buf),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.write_all(buf),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Buffer::File(w) => w.flush(),
                Buffer::Redirect(w) => w.flush(),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.flush(),
            }
        }
    }

    impl WriteColor for Buffer {
        fn supports_color(&self) -> bool {
            match self {
                Buffer::File(w) => w.supports_color(),
                Buffer::Redirect(w) => w.supports_color(),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.supports_color(),
            }
        }

        fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
            match self {
                Buffer::File(w) => w.set_color(spec),
                Buffer::Redirect(w) => w.set_color(spec),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.set_color(spec),
            }
        }

        fn reset(&mut self) -> std::io::Result<()> {
            match self {
                Buffer::File(w) => w.reset(),
                Buffer::Redirect(w) => w.reset(),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.reset(),
            }
        }

        fn is_synchronous(&self) -> bool {
            match self {
                Buffer::File(w) => w.is_synchronous(),
                Buffer::Redirect(w) => w.is_synchronous(),
                Buffer::Stdout(w) | Buffer::Stderr(w) => w.is_synchronous(),
            }
        }
    }
}

impl Buffer {
    pub fn new(download: bool, output: Option<&Path>, is_stdout_tty: bool) -> io::Result<Self> {
        log::trace!("is_stdout_tty: {is_stdout_tty}");
        Ok(if download {
            Buffer::stderr()
        } else if let Some(output) = output {
            log::trace!("creating file {output:?}");
            let file = std::fs::File::create(output)?;
            Buffer::file(file)
        } else if is_stdout_tty {
            Buffer::stdout()
        } else {
            Buffer::redirect()
        })
    }

    pub fn print(&mut self, s: impl AsRef<[u8]>) -> io::Result<()> {
        self.write_all(s.as_ref())
    }

    pub fn guess_pretty(&self) -> Pretty {
        if test_default_color() {
            Pretty::All
        } else if test_pretend_term() {
            Pretty::Format
        } else if self.is_terminal() {
            // Based on termcolor's logic for ColorChoice::Auto
            if cfg!(test) {
                Pretty::All
            } else if var_os("NO_COLOR").is_some_and(|val| !val.is_empty()) {
                Pretty::Format
            } else {
                match var_os("TERM") {
                    Some(term) if term == "dumb" => Pretty::Format,
                    Some(_) => Pretty::All,
                    None if cfg!(windows) => Pretty::All,
                    None => Pretty::Format,
                }
            }
        } else {
            Pretty::None
        }
    }
}
