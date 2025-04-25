use bytes::Bytes;

use crate::models::configuration::File;

pub trait Downloader {
    fn new() -> impl Downloader;

    fn build_url_and_sha256(
        &self,
        configuration: &File,
        os : &str,
        arch: &str,
    ) -> impl std::future::Future<Output = Result<(String, String), Box<dyn std::error::Error>>> + Send;

    fn download_and_validate_sha256(
        &self,
        url: &str,
        sha256: &str,
    ) -> impl std::future::Future<Output = Result<Bytes, Box<dyn std::error::Error>>> + Send;
}