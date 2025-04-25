use std::path::PathBuf;

use crate::models::download_marker::MarkerFile;

pub trait MarkerManager {
    fn new() -> impl MarkerManager;
    async fn invoke_if_different<F, Fut>(
        &self,
        path: PathBuf,
        marker_file: &MarkerFile,
        callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>>;
}
