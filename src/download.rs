use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;

use anyhow::Result;
use atty::Stream;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use mime2ext::mime2ext;
use reqwest::header::{HeaderMap, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Response;

use crate::regex;

fn get_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn get_file_name(response: &Response, orig_url: &reqwest::Url) -> String {
    fn from_header(response: &Response) -> Option<String> {
        let quoted = regex!("filename=\"([^\"]*)\"");
        // Against the spec, but used by e.g. Github's zip downloads
        let unquoted = regex!("filename=([^;=\"]*)");

        let header = response.headers().get(CONTENT_DISPOSITION)?.to_str().ok()?;
        let caps = quoted
            .captures(header)
            .or_else(|| unquoted.captures(header))?;
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

pub fn get_file_size(path: &Option<String>) -> Option<u64> {
    match path {
        Some(path) => Some(std::fs::metadata(path).ok()?.len()),
        _ => None,
    }
}

fn exists(file_name: &str) -> bool {
    fs::metadata(&file_name).is_ok()
}

/// Find a file name that doesn't exist yet.
fn generate_file_name(file_name: String) -> String {
    if !exists(&file_name) {
        return file_name;
    }
    let mut suffix: u32 = 1;
    loop {
        let candidate = format!("{}-{}", file_name, suffix);
        if !exists(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

pub async fn download_file(
    mut response: reqwest::Response,
    file_name: Option<String>,
    // If we fall back on taking the filename from the URL it has to be the
    // original URL, before redirects. That's less surprising and matches
    // HTTPie. Hence this argument.
    orig_url: &reqwest::Url,
    resume: bool,
    quiet: bool,
) -> Result<()> {
    let (mut buffer, dest_name): (Box<dyn IoWrite>, String) = match file_name {
        Some(file_name) => (
            Box::new(
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(resume)
                    .open(&file_name)?,
            ),
            file_name,
        ),
        None if atty::is(Stream::Stdout) => {
            let file_name = get_file_name(&response, &orig_url);
            // TODO: do not avoid name conflict if `continue` flag is specified
            let file_name = generate_file_name(file_name);
            (
                Box::new(
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(resume)
                        .open(&file_name)?,
                ),
                file_name,
            )
        }
        None => (Box::new(std::io::stdout()), "stdout".into()),
    };

    let pb = if quiet {
        None
    } else {
        match get_content_length(&response.headers()) {
            Some(content_length) => {
                eprintln!(
                    "Downloading {} to \"{}\"",
                    HumanBytes(content_length),
                    dest_name
                );
                let template = "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes} {bytes_per_sec} ETA {eta}";
                Some(
                    ProgressBar::new(content_length).with_style(
                        ProgressStyle::default_bar()
                            .template(template)
                            .progress_chars("#>-"),
                    ),
                )
            }
            None => {
                eprintln!("Downloading to \"{}\"", dest_name);
                Some(
                    ProgressBar::new_spinner().with_style(ProgressStyle::default_bar().template(
                        "{spinner:.green} [{elapsed_precise}] {bytes} {bytes_per_sec} {msg}",
                    )),
                )
            }
        }
    };

    let mut downloaded = 0;
    while let Some(chunk) = response.chunk().await? {
        buffer.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if let Some(pb) = &pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = &pb {
        pb.finish_with_message("Done");
    }

    Ok(())
}
