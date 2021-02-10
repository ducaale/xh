use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};

use anyhow::{anyhow, Context, Result};
use atty::Stream;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use mime2ext::mime2ext;
use reqwest::{
    blocking::Response,
    header::{HeaderMap, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE},
    StatusCode,
};

use crate::regex;

fn get_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn get_file_name(response: &Response, orig_url: &reqwest::Url) -> String {
    fn from_header(response: &Response) -> Option<String> {
        regex!(QUOTED = "filename=\"([^\"]*)\"");
        // Against the spec, but used by e.g. Github's zip downloads
        regex!(UNQUOTED = "filename=([^;=\"]*)");

        let header = response.headers().get(CONTENT_DISPOSITION)?.to_str().ok()?;
        let caps = QUOTED
            .captures(header)
            .or_else(|| UNQUOTED.captures(header))?;
        Some(caps[1].to_string())
    }

    fn from_url(url: &reqwest::Url) -> Option<String> {
        let last_seg = url
            .path_segments()?
            .rev()
            .find(|segment| !segment.is_empty())?;
        Some(last_seg.to_string())
    }

    fn guess_extension(response: &Response) -> Option<&'static str> {
        let mimetype = response.headers().get(CONTENT_TYPE)?.to_str().ok()?;
        mime2ext(mimetype)
    }

    let mut filename = from_header(response)
        .or_else(|| from_url(orig_url))
        .unwrap_or_else(|| "index".to_string());

    filename = filename.trim().trim_start_matches('.').to_string();

    if !filename.contains('.') {
        if let Some(extension) = guess_extension(response) {
            filename.push('.');
            filename.push_str(extension);
        }
    }

    filename
}

pub fn get_file_size(path: Option<&str>) -> Option<u64> {
    Some(fs::metadata(path?).ok()?.len())
}

/// Find a file name that doesn't exist yet.
fn open_new_file(file_name: String) -> io::Result<(String, File)> {
    fn try_open_new(file_name: &str) -> io::Result<Option<File>> {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file_name)
        {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(None),
            Err(err) => Err(err),
        }
    }
    if let Some(file) = try_open_new(&file_name)? {
        return Ok((file_name, file));
    }
    for suffix in 1..u32::MAX {
        let candidate = format!("{}-{}", file_name, suffix);
        if let Some(file) = try_open_new(&candidate)? {
            return Ok((candidate, file));
        }
    }
    panic!("Could not create file after unreasonable number of attempts");
}

// https://github.com/httpie/httpie/blob/84c7327057/httpie/downloads.py#L44
// https://tools.ietf.org/html/rfc7233#section-4.2
fn total_for_content_range(header: &str, expected_start: u64) -> Result<u64> {
    regex!(RE_RANGE =
        r"^bytes (?P<first_byte_pos>\d+)-(?P<last_byte_pos>\d+)"
        r"/(?:\*|(?P<complete_length>\d+))$"
    );
    let caps = RE_RANGE
        .captures(header)
        // Could happen if header uses unit other than bytes
        .ok_or_else(|| anyhow!("Can't parse Content-Range header, can't resume download"))?;
    let first_byte_pos: u64 = caps
        .name("first_byte_pos")
        .unwrap()
        .as_str()
        .parse()
        .context("Can't parse Content-Range first_byte_pos")?;
    let last_byte_pos: u64 = caps
        .name("last_byte_pos")
        .unwrap()
        .as_str()
        .parse()
        .context("Can't parse Content-Range last_byte_pos")?;
    let complete_length: Option<u64> = caps
        .name("complete_length")
        .map(|num| {
            num.as_str()
                .parse()
                .context("Can't parse Content-Range complete_length")
        })
        .transpose()?;
    // Note that last_byte_pos must be strictly less than complete_length
    // If first_byte_pos == last_byte_pos exactly one byte is sent
    if first_byte_pos > last_byte_pos {
        return Err(anyhow!("Invalid Content-Range: {:?}", header));
    }
    if let Some(complete_length) = complete_length {
        if last_byte_pos >= complete_length {
            return Err(anyhow!("Invalid Content-Range: {:?}", header));
        }
        if complete_length != last_byte_pos + 1 {
            return Err(anyhow!("Content-Range has wrong end: {:?}", header));
        }
    }
    if expected_start != first_byte_pos {
        return Err(anyhow!("Content-Range has wrong start: {:?}", header));
    }
    Ok(last_byte_pos + 1)
}

const BAR_TEMPLATE: &str =
    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes} {bytes_per_sec} ETA {eta}";
const SPINNER_TEMPLATE: &str = "{spinner:.green} [{elapsed_precise}] {bytes} {bytes_per_sec} {msg}";

pub fn download_file(
    mut response: Response,
    file_name: Option<String>,
    // If we fall back on taking the filename from the URL it has to be the
    // original URL, before redirects. That's less surprising and matches
    // HTTPie. Hence this argument.
    orig_url: &reqwest::Url,
    mut resume: Option<u64>,
    quiet: bool,
) -> Result<()> {
    if resume.is_some() && response.status() != StatusCode::PARTIAL_CONTENT {
        resume = None;
    }

    let mut buffer: Box<dyn io::Write>;
    let dest_name: String;

    if let Some(file_name) = file_name {
        let mut open_opts = OpenOptions::new();
        open_opts.write(true).create(true);
        if resume.is_some() {
            open_opts.append(true);
        } else {
            open_opts.truncate(true);
        }

        dest_name = file_name;
        buffer = Box::new(open_opts.open(&dest_name)?);
    } else if atty::is(Stream::Stdout) {
        let (new_name, handle) = open_new_file(get_file_name(&response, &orig_url))?;
        dest_name = new_name;
        buffer = Box::new(handle);
    } else {
        dest_name = "<stdout>".into();
        buffer = Box::new(io::stdout());
    }

    let starting_length: u64;
    let total_length: Option<u64>;
    if let Some(resume) = resume {
        let header = response
            .headers()
            .get(CONTENT_RANGE)
            .ok_or_else(|| anyhow!("Missing Content-Range header"))?
            .to_str()
            .map_err(|_| anyhow!("Bad Content-Range header"))?;
        starting_length = resume;
        total_length = Some(total_for_content_range(header, starting_length)?);
    } else {
        starting_length = 0;
        total_length = get_content_length(&response.headers());
    }

    let pb = if quiet {
        None
    } else if let Some(total_length) = total_length {
        eprintln!(
            "Downloading {} to {:?}",
            HumanBytes(total_length - starting_length),
            dest_name
        );
        let style = ProgressStyle::default_bar()
            .template(BAR_TEMPLATE)
            .progress_chars("#>-");
        Some(ProgressBar::new(total_length).with_style(style))
    } else {
        eprintln!("Downloading to {:?}", dest_name);
        let style = ProgressStyle::default_bar().template(SPINNER_TEMPLATE);
        Some(ProgressBar::new_spinner().with_style(style))
    };
    if let Some(pb) = &pb {
        pb.set_position(starting_length);
    }

    match pb {
        Some(ref pb) => {
            copy_largebuf(&mut pb.wrap_read(response), &mut buffer)?;
            pb.finish_with_message("Done");
        }
        None => {
            copy_largebuf(&mut response, &mut buffer)?;
        }
    }

    Ok(())
}

const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;

/// io::copy, but with a larger buffer size.
///
/// io::copy's buffer is just 8 KiB. This noticeably slows down fast
/// large downloads, especially with a progress bar.
///
/// This one's size of 64 KiB was chosen because that makes it competitive
/// with the old implementation, which repeatedly called .chunk().await.
///
/// Tests were done by running `ht -o /dev/null [-d]` on a two-gigabyte file
/// served locally by `python3 -m http.server`. Results may vary.
fn copy_largebuf(reader: &mut impl io::Read, writer: &mut impl Write) -> io::Result<()> {
    let mut buf = vec![0; DOWNLOAD_BUFFER_SIZE];
    let mut buf = buf.as_mut_slice();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(len) => writer.write_all(&buf[..len])?,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parsing() {
        let expected = vec![
            (2, "bytes 2-5/6", Some(6)),
            (2, "bytes 2-5/*", Some(6)),
            (5, "bytes 5-5/6", Some(6)),
            (2, "bytes 3-5/6", None),
            (2, "bytes 1-5/6", None),
            (2, "bytes 2-4/6", None),
            (2, "bytes 2-6/6", None),
        ];
        for (start, header, result) in expected {
            assert_eq!(total_for_content_range(header, start).ok(), result);
        }
    }
}
