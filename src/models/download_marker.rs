#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MarkerFile {
    pub sha256: String,
    pub url: String,
}