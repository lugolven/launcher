use bytes::Bytes;
use sha2::Digest;
use std::{
    collections::BTreeMap,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{self, PathBuf},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Configuration {
    name: String,
    version: String,
    #[serde(rename = "urlPattern")]
    url_pattern: String,
    platforms: ConfigurationPlatforms,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stripPrefix")]
    strip_prefix: Option<String>,
    compression: ConfigurationCompression,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ConfigurationCompression {
    #[serde(rename = "type")]
    compression_type: ConfigurationCompressionType,
    #[serde(rename = "stripPrefix")]
    strip_prefix: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum ConfigurationCompressionType {
    Zip,
}

type ConfigurationPlatforms = BTreeMap<String, ConfigurationPlatformOS>;
type ConfigurationPlatformOS = BTreeMap<String, ConfigurationPlatformOSArchitecture>;
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
struct ConfigurationPlatformOSArchitecture {
    sha256: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
struct MarkerFile {
    sha256: String,
    url: String,
}

async fn read_json_file(path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    let file = tokio::fs::read(path).await?;
    let content = String::from_utf8(file)?;
    // remove the first line
    let content = content.lines().skip(1).collect::<Vec<_>>().join("\n");
    let base_file: Configuration = serde_json::from_str(&content)?;
    Ok(base_file)
}

static CACHE_LOCATION: &str = "~/.launcher";

async fn download_file(url: &str) -> Result<Bytes, Box<dyn std::error::Error>> {
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;
    Ok(bytes)
}

async fn download_and_validate_sha256(
    url: &str,
    sha256: &str,
) -> Result<Bytes, Box<dyn std::error::Error>> {
    let bytes = download_file(url).await?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    let hash = format!("{:x}", result);
    if hash != sha256 {
        return Err(format!("SHA256 mismatch: expected {}, got {}", sha256, hash).into());
    }
    Ok(bytes)
}

static ARCH_MAPPING: [(&str, &str); 2] = [("x86_64", "amd64"), ("aarch64", "arm64")];

async fn build_url_and_sha256(
    configuration: &Configuration,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let arch = ARCH_MAPPING
        .iter()
        .find(|(k, _)| *k == arch)
        .map(|(_, v)| *v)
        .unwrap_or(arch);
    let platform = configuration
        .platforms
        .get(os)
        .ok_or("Platform not found")?;
    let architecture = platform.get(arch).ok_or("Architecture not found")?;
    let url = configuration
        .url_pattern
        .replace("{{version}}", &configuration.version)
        .replace("{{os}}", os)
        .replace("{{arch}}", arch);
    Ok((url, architecture.sha256.clone()))
}

async fn extract_to_disk(
    compression: &ConfigurationCompression,
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
        ConfigurationCompressionType::Zip => {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(content.to_vec()))
                .or_else(|e| {
                    Err::<_, Box<dyn std::error::Error>>(
                        format!("Failed to read zip archive: {}", e).into(),
                    )
                })?;

            // unzip in folder
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).or_else(|e| {
                    Err::<_, Box<dyn std::error::Error>>(
                        format!("Failed to get by index {}, {}", i, e).into(),
                    )
                })?;
                let outpath = folder.join(file.name());
                if file.name().ends_with('/') {
                    tokio::fs::create_dir_all(&outpath).await.or_else(|e| {
                        Err::<_, Box<dyn std::error::Error>>(
                            format!("Failed to create dir {}, {}", outpath.display(), e).into(),
                        )
                    })?;
                } else {
                    if let Some(parent) = outpath.parent() {
                        tokio::fs::create_dir_all(parent).await.or_else(|e| {
                            Err::<_, Box<dyn std::error::Error>>(
                                format!("Failed to create dir {}, {}", parent.display(), e).into(),
                            )
                        })?;
                    }
                    let mut outfile = tokio::fs::File::create(&outpath).await.or_else(|e| {
                        Err::<_, Box<dyn std::error::Error>>(
                            format!("Failed to create file {}, {}", outpath.display(), e).into(),
                        )
                    })?;
                    let mut buffer = Vec::new();
                    file.read_to_end(&mut buffer).or_else(|e| {
                        Err::<_, Box<dyn std::error::Error>>(
                            format!("Failed to read file {}, {}", file.name(), e).into(),
                        )
                    })?;
                    outfile.write_all(&buffer).await.or_else(|e| {
                        Err::<_, Box<dyn std::error::Error>>(
                            format!("Failed to write file {}, {}", outpath.display(), e).into(),
                        )
                    })?;
                }
            }
        }
    }

    let mut permissions = tokio::fs::metadata(&executable_path).await?.permissions();
    permissions.set_mode(0o755);
    tokio::fs::set_permissions(&executable_path, permissions)
        .await
        .or_else(|e| {
            Err::<_, Box<dyn std::error::Error>>(
                format!(
                    "Failed to set permissions {}, {}",
                    executable_path.display(),
                    e
                )
                .into(),
            )
        })?;
    Ok(())
}

fn marker_file_json_content(
    sha256: &str,
    url: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let marker_file = MarkerFile {
        sha256: sha256.to_string(),
        url: url.to_string(),
    };  
    let json = serde_json::to_vec(&marker_file)?;
    Ok(json)
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

    let configuration = read_json_file(&file).await?;
    let binding = shellexpand::tilde(CACHE_LOCATION);
    let cache_path = path::Path::new(binding.as_ref());
    let command_cache_base = cache_path.join(&configuration.name);

    let download_path = command_cache_base.join("downloaded");
    let executable_path = download_path.join(&configuration.name);
    let sha256_marker_file = command_cache_base.join("sha256");

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

    let (url, sha256) = build_url_and_sha256(&configuration).await?;
    
    let marker_file_expected_content = marker_file_json_content(&sha256, &url)?;
    let need_redownload = if tokio::fs::metadata(&executable_path).await.is_err() {
        true
    } else {
        let mut sha256_file = tokio::fs::File::open(&sha256_marker_file).await?;
        let mut sha256_content = Vec::new();
        sha256_file.read_to_end(&mut sha256_content).await?;
        sha256_content != marker_file_expected_content
    };

    if need_redownload {
        eprint!("Downloading {}...", configuration.name);
        // remove the sha256 file if it exists
        if tokio::fs::metadata(&sha256_marker_file).await.is_ok() {
            tokio::fs::remove_file(&sha256_marker_file)
                .await
                .or_else(|e| {
                    Err::<_, Box<dyn std::error::Error>>(
                        format!(
                            "Failed to remove sha256 file {}, {}",
                            sha256_marker_file.display(),
                            e
                        )
                        .into(),
                    )
                })?;
        }
        
        let content: Bytes = download_and_validate_sha256(&url, &sha256).await?;

        extract_to_disk(
            &configuration.compression,
            &content,
            &download_path,
            &executable_path,
        )
        .await?;

        // write the sha256 to the file
        let mut sha256_file = tokio::fs::File::create(&sha256_marker_file)
            .await
            .or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!(
                        "Failed to create sha256 file {}, {}",
                        sha256_marker_file.display(),
                        e
                    )
                    .into(),
                )
            })?;

        sha256_file
            .write_all(&marker_file_expected_content)
            .await
            .or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!(
                        "Failed to write sha256 file {}, {}",
                        sha256_marker_file.display(),
                        e
                    )
                    .into(),
                )
            })?;

        eprintln!("Done!");
    }

    // exec executable_path with args of the current process and forward stdin, stdout, and stderr
    let args = std::env::args().skip(2).collect::<Vec<_>>();
    println!("Executing: {:?}", args);
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
