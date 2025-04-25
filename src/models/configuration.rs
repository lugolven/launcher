use std::collections::BTreeMap;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct File {
    pub name: String,
    pub version: String,
    #[serde(rename = "urlPattern")]
    pub url_pattern: String,
    pub platforms: Platforms,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stripPrefix")]
    pub strip_prefix: Option<String>,
    pub compression: Compression,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Compression {
    #[serde(rename = "type")]
    pub compression_type: CompressionType,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionType {
    Zip,
}

pub type Platforms = BTreeMap<String, PlatformOS>;
pub type PlatformOS = BTreeMap<String, PlatformOSArchitecture>;
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlatformOSArchitecture {
    pub sha256: String,
}
