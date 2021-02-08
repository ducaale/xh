use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::path;

use anyhow::Result;
use atty::Stream;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::header::{HeaderMap, CONTENT_LENGTH};

fn get_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

// TODO: avoid name conflict unless `continue` flag is specified
fn get_file_name(response: &reqwest::Response) -> String {
    lazy_static::lazy_static! {
        static ref RE: Regex = Regex::new("filename=\"([^\"]*)\"").unwrap();
    }

    response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| RE.captures(value))
        .and_then(|caps| {
            caps[1]
                .split(path::is_separator)
                .last()
                .map(ToString::to_string)
        })
        .or_else(|| {
            response
                .url()
                .path_segments()
                .and_then(|segments| segments.last().map(ToString::to_string))
        })
        .map(|name| name.trim().trim_start_matches('.').to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| String::from("index.html"))
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
            let file_name = get_file_name(&response);
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
