use bytes::Bytes;

use std::path::PathBuf;

pub trait Extractor {
    fn new() -> impl Extractor;

    fn extract_to_disk(
        &self,
        content: &Bytes,
        folder: &PathBuf,
        executable_path: &PathBuf,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send;
}
