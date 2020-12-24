use indicatif::{HumanBytes, ProgressBar, ProgressStyle};

use crate::utils::get_content_length;

fn get_file_name(response: &reqwest::Response) -> &str {
    if let Some(_) = response.headers().get(reqwest::header::CONTENT_DISPOSITION) {
        todo!()
    } else {
        response.url().path_segments().unwrap().last().unwrap()
    }
}

pub async fn download_file(mut response: reqwest::Response) {
    let file_name = get_file_name(&response);
    let total_size = get_content_length(&response.headers()).unwrap();

    eprintln!(
        "Downloading {} to \"{}\"",
        HumanBytes(total_size),
        file_name
    );

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes} {bytes_per_sec} ETA {eta}")
        .progress_chars("#>-"));

    let mut downloaded = 0;
    while let Some(chunk) = response.chunk().await.unwrap() {
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("downloaded");
}
