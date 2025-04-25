use bytes::Bytes;
use models::download_marker::MarkerFile;
use std::path::{self, PathBuf};
use tokio::process::Command;

pub mod providers;
use providers::{
    downloader::Downloader, extractor::Extractor, file_marker_manager::FileMarkerManager,
    marker_manager::MarkerManager, zip_extractor::ZipExtractor,
};

pub mod models;
use crate::models::configuration::{Compression, CompressionType, File};

use crate::providers::unauthenticated_downloader::UnauthenticatedDownloader;

async fn read_configuration(path: &str) -> Result<File, Box<dyn std::error::Error>> {
    let file = tokio::fs::read(path).await?;
    let content = String::from_utf8(file)?;
    let base_file: File = serde_yaml::from_str(&content)?;
    Ok(base_file)
}

static CACHE_LOCATION: &str = "~/.launcher";

async fn extract_to_disk(
    compression: &Compression,
    content: &Bytes,
    folder: &PathBuf,
    executable_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if tokio::fs::metadata(folder).await.is_ok() {
        tokio::fs::remove_dir_all(folder.to_str().unwrap())
            .await
            .or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!("Failed to remove dir {}, {}", folder.display(), e).into(),
                )
            })?;
    }
    match compression.compression_type {
        CompressionType::Zip => {
            let extractor = ZipExtractor::new();
            extractor
                .extract_to_disk(content, folder, executable_path)
                .await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!(
            "Usage: {} <path to json file>",
            std::env::args().next().unwrap()
        );
        std::process::exit(1);
    });

    let downloader = UnauthenticatedDownloader::new();
    let marker_manager = FileMarkerManager::new();

    let configuration = read_configuration(&file).await?;
    let binding = shellexpand::tilde(CACHE_LOCATION);
    let cache_path = path::Path::new(binding.as_ref());
    let command_cache_base = cache_path.join(&configuration.name);

    let download_path = command_cache_base.join("downloaded");
    let executable_path = download_path.join(&configuration.name);
    let sha256_marker_path = command_cache_base.join("sha256");

    if tokio::fs::metadata(&download_path).await.is_err() {
        tokio::fs::create_dir_all(&download_path)
            .await
            .or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!(
                        "Failed to create download dir {}, {}",
                        download_path.display(),
                        e
                    )
                    .into(),
                )
            })?;
    }

    let (url, sha256) = downloader
        .build_url_and_sha256(&configuration, std::env::consts::OS, std::env::consts::ARCH)
        .await?;

    marker_manager.invoke_if_different(
        sha256_marker_path,
        &MarkerFile {
            sha256: sha256.clone(),
            url: url.clone(),
        },
        || async {
            eprint!("Downloading {}...", configuration.name);

            let content: Bytes = downloader
                .download_and_validate_sha256(&url, &sha256)
                .await?;

            extract_to_disk(
                &configuration.compression,
                &content,
                &download_path,
                &executable_path,
            )
            .await?;
            eprintln!("Done!");

            Ok(())
        },
    ).await?;

    let args = std::env::args().skip(2).collect::<Vec<_>>();
    let mut command = Command::new(executable_path);
    command.args(args);
    command.stdin(std::process::Stdio::inherit());
    command.stdout(std::process::Stdio::inherit());
    command.stderr(std::process::Stdio::inherit());

    for (key, value) in std::env::vars() {
        command.env(key, value);
    }

    let status = command.status().await?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}