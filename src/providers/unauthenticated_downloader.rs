use std::collections::HashMap;
use sha2::Digest;
use crate::models::configuration::File;
use bytes::Bytes;

static ARCH_MAPPING: std::sync::LazyLock<HashMap<&str, &str>> = std::sync::LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert("x86_64", "amd64");
    map.insert("aarch64", "arm64");
    map
});

pub struct UnauthenticatedDownloader {}

impl crate::providers::downloader::Downloader for UnauthenticatedDownloader {
    fn new() -> impl super::downloader::Downloader {
        UnauthenticatedDownloader{}
    }

    async fn build_url_and_sha256(
        &self,
        configuration: &File,
        os: &str,
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

    async fn download_and_validate_sha256(
        &self,
        url: &str,
        sha256: &str,
    ) -> Result<Bytes, Box<dyn std::error::Error>> {
        let response = reqwest::get(url).await?;
        let bytes = response.bytes().await?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        let hash = format!("{:x}", result);
        if hash != sha256 {
            return Err(format!("SHA256 mismatch: expected {}, got {}", sha256, hash).into());
        }
        Ok(bytes)
    }
}


#[cfg(test)]
mod tests {
    use crate::providers::downloader::Downloader;

    use super::*;
    use iron::{status, Iron, IronResult, Listening, Request, Response};

    use std::net::TcpListener;
    use rand::rng;
    use rand::seq::SliceRandom;

    fn get_available_port() -> Option<u16> {
        let ports = 8000..9000;
        let mut rng = rng();
        let mut ports: Vec<u16> = ports.collect();
        ports.shuffle(&mut rng);
        ports.iter().find(|&&port| port_is_available(port)).copied()
    }

    fn port_is_available(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    fn mock_server(content: &'static [u8]) -> (Listening, u16) {
        let port = get_available_port().expect("No available port found");

        let handler = move |_req: &mut Request| -> IronResult<Response> {
            Ok(Response::with((status::Ok, content)))
        };
        
        match Iron::new(handler).http(("127.0.0.1", port)) {
            Ok(listener) => (listener, port),
            Err(e) => panic!("Failed to start server: {}", e),
        } 
    }

    #[tokio::test]
    async fn test_download_and_validate_sha256_success() {
        // Arrange
        let content = b"test content";
        let mut hasher = sha2::Sha256::new();
        hasher.update(content);
        let sha256 = format!("{:x}", hasher.finalize());

        let (mut server, port) = mock_server(content);
        let url = format!("http://127.0.0.1:{}", port);

        let downloader = UnauthenticatedDownloader::new();

        // Act
        let result = downloader.download_and_validate_sha256(&url, &sha256).await;

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::from_static(content));

        // Clean up
        server.close().unwrap();
    }

    #[tokio::test]
    async fn test_download_and_validate_sha256_mismatch() {
        // Arrange
        let content = b"test content";
        let incorrect_sha256 = "incorrectsha256";

        let (mut server, port) = mock_server(content);
        let url = format!("http://127.0.0.1:{}", port);

        let downloader = UnauthenticatedDownloader::new();

        // Act
        let result = downloader.download_and_validate_sha256(&url, incorrect_sha256).await;

        // Assert
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("SHA256 mismatch"));
        
        // Clean up
        server.close().unwrap();
    }
}