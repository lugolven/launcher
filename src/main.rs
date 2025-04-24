use bytes::Bytes;
use models::download_marker::MarkerFile;
use sha2::Digest;
use std::{
    collections::HashMap,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{self, PathBuf},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
pub mod models;

use crate::models::configuration::{
    File,
    Compression,
    CompressionType
};


async fn read_yaml_file(path: &str) -> Result<File, Box<dyn std::error::Error>> {
    let file = tokio::fs::read(path).await?;
    let content = String::from_utf8(file)?;
    let base_file: File = serde_yaml::from_str(&content)?;
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
static ARCH_MAPPING: std::sync::LazyLock<HashMap<&str, &str>> = std::sync::LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert("x86_64", "amd64");
    map.insert("aarch64", "arm64");
    map
});

async fn build_url_and_sha256(
    configuration: &File,
    os : &str,
    arch: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    
    let arch = ARCH_MAPPING.get(arch).ok_or("Architecture not found")?;
    let platform = configuration
        .platforms
        .get(os)
        .ok_or("Platform not found")?;
    let architecture = platform.get(*arch).ok_or("Architecture not found")?;
    let url = configuration
        .url_pattern
        .replace("{{version}}", &configuration.version)
        .replace("{{os}}", os)
        .replace("{{arch}}", arch);
    Ok((url, architecture.sha256.clone()))
}

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
    let yaml = serde_yaml::to_string(&marker_file)?;

    Ok(yaml.as_bytes().to_vec())
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

    let configuration = read_yaml_file(&file).await?;
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
    let (url, sha256) = build_url_and_sha256(&configuration, std::env::consts::OS, std::env::consts::ARCH).await?;
    
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


#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, net::TcpListener};

    use crate::models::configuration::PlatformOSArchitecture;

    use super::*;
    use iron::{status, Iron, IronResult, Request, Response};
    use tokio::fs;

    fn get_available_port() -> Option<u16> {
        (8000..9000)
            .find(|port| port_is_available(*port))
    }
    
    fn port_is_available(port: u16) -> bool {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    #[tokio::test]
    async fn test_read_yaml_file() {
        // Arrange
        let test_file_path = "test_config.json";
        let test_content = r#"#! /bin/launcher
        name: test_binary
        version: 1.0.0
        urlPattern: https://example.com/{{version}}/{{os}}/{{arch}}.zip
        platforms:
            linux:
                arm64:
                    sha256: dummysha256
        compression:
            type: zip
"#;

        fs::write(test_file_path, test_content).await.unwrap();

        // Act
        let config = read_yaml_file(test_file_path).await.unwrap();

        // Assert
        assert_eq!(config.name, "test_binary");
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.url_pattern, "https://example.com/{{version}}/{{os}}/{{arch}}.zip");
        fs::remove_file(test_file_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_file() {
        // Arrange
        fn hello_world(_: &mut Request) -> IronResult<Response> {
            Ok(Response::with((status::Ok, "Hello World!")))
        }
        let port = get_available_port().unwrap();
        let mut server = Iron::new(hello_world).http(format!("localhost:{}", port)).unwrap();
        let url = format!("http://localhost:{}", port);
        
        // Act
        let result = download_file(&url).await;

        // Assert
        assert!(result.is_ok());
        server.close().unwrap();
    }

    #[tokio::test]
    async fn test_download_and_validate_sha256() {
        // Arrange
        fn hello_world(_: &mut Request) -> IronResult<Response> {
            Ok(Response::with((status::Ok, "Hello World!")))
        }
        let port = get_available_port().unwrap();
        let mut server = Iron::new(hello_world).http(format!("localhost:{}", port)).unwrap();
        let url = format!("http://localhost:{}", port);

        let sha256 = "d41d8cd98f00b204e9800998ecf8427e"; // Dummy SHA256 for testing
    
        // Act
        let result = download_and_validate_sha256(&url, sha256).await;
        
        // Assert
        assert!(result.is_err()); // Should fail due to SHA256 mismatch
        server.close().unwrap();
    }

    #[tokio::test]
    async fn test_build_url_and_sha256() {
        // Arrange
        let config = File {
            name: "test_binary".to_string(),
            version: "1.0.0".to_string(),
            url_pattern: "https://example.com/{{version}}/{{os}}/{{arch}}.zip".to_string(),
            platforms: {
                let mut platforms = BTreeMap::new();
                let mut arch_map = BTreeMap::new();
                arch_map.insert("arm64".to_string(), PlatformOSArchitecture {
                    sha256: "dummysha256".to_string(),
                });
                platforms.insert("linux".to_string(), arch_map);
                platforms
            },
            strip_prefix: None,
            compression: Compression {
                compression_type: CompressionType::Zip
            },
        };

        // Act
        let result = build_url_and_sha256(&config, "linux", "aarch64").await;
        
        // Assert
        if result.is_err() {
            panic!("Error: {:?}", result.err());
        }
        let (url, sha256) = result.unwrap();
        assert_eq!(url, "https://example.com/1.0.0/linux/arm64.zip");
        assert_eq!(sha256, "dummysha256");
    }

    #[tokio::test]
    async fn test_marker_file_json_content() {
        // Arrange
        let sha256 = "dummysha256";
        let url = "https://example.com/test.zip";
        

        // Act
        let result = marker_file_json_content(sha256, url);

        // Assert
        assert!(result.is_ok());
        let json = result.unwrap();
        let expected_json = r#"sha256: dummysha256
url: https://example.com/test.zip
"#;
        assert_eq!(String::from_utf8(json).unwrap(), expected_json);
    }
}