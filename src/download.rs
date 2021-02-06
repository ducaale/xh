use std::fs::OpenOptions;
use std::io::Write as IoWrite;

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
    let fallback = response.url().path_segments().unwrap().last().unwrap();

    if let Some(value) = response.headers().get(reqwest::header::CONTENT_DISPOSITION) {
        let re = Regex::new("filename=\"(.*)\"").unwrap();
        if let Some(caps) = re.captures(value.to_str().unwrap()) {
            caps[1].to_string()
        } else {
            fallback.to_string()
        }
    } else {
        fallback.to_string()
    }
}

pub fn get_file_size(path: &Option<String>) -> Option<u64> {
    match path {
        Some(path) => Some(std::fs::metadata(path).ok()?.len()),
        _ => None,
    }
}

pub async fn download_file(
    mut response: reqwest::Response,
    file_name: Option<String>,
    resume: bool,
    quiet: bool,
) {
    let (mut buffer, dest_name): (Box<dyn IoWrite>, String) = match file_name {
        Some(file_name) => (
            Box::new(
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(resume)
                    .open(&file_name)
                    .unwrap(),
            ),
            file_name,
        ),
        None if atty::is(Stream::Stdout) => {
            let file_name = get_file_name(&response);
            (
                Box::new(
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(resume)
                        .open(&file_name)
                        .unwrap(),
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
    while let Some(chunk) = response.chunk().await.unwrap() {
        buffer.write_all(&chunk).unwrap();
        downloaded += chunk.len() as u64;
        if let Some(pb) = &pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = &pb {
        pb.finish_with_message("Done");
    }
}
