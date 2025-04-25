use std::path::PathBuf;

use tokio::io::AsyncReadExt;

use super::marker_manager::MarkerManager;

pub struct FileMarkerManager {

}

impl MarkerManager for FileMarkerManager {
    fn new() -> impl MarkerManager {
        FileMarkerManager { }
    }

    async fn invoke_if_different<F, Fut>(
        &self,
        path: PathBuf,
        marker_file: &crate::models::download_marker::MarkerFile,
        callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>>,
    {
        let yaml = serde_yaml::to_string(&marker_file)?;
        let yaml_bytes = yaml.as_bytes();

        let (is_different, exists) = if tokio::fs::metadata(&path).await.is_err() {
            (true, false)
        } else {
            let mut sha256_file = tokio::fs::File::open(&path).await?;
            let mut sha256_content = Vec::new();
            sha256_file.read_to_end(&mut sha256_content).await?;
            (sha256_content != yaml_bytes, true)
        };

        if is_different {
            if exists {
                tokio::fs::remove_file(&path).await.or_else(|e| {
                    Err::<_, Box<dyn std::error::Error>>(
                        format!(
                            "Failed to remove sha256 file {}, {}",
                            path.display(),
                            e
                        )
                        .into(),
                    )
                })?;
            }

            callback().await?;

            tokio::fs::write(&path, yaml_bytes).await.or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!("Failed to write sha256 file {}, {}", path.display(), e).into(),
                )
            })?;
        }
        Ok(())
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::download_marker::MarkerFile;
    use tokio::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_invoke_if_different_creates_file_when_missing() {
        // Arrange
        let temp_dir = tempdir().unwrap();
        let marker_path = temp_dir.path().join("marker.yaml");
        let marker_file = MarkerFile {
            sha256: "dummysha256".to_string(),
            url: "https://example.com/test.zip".to_string(),
        };
        let manager = FileMarkerManager {};

        let callback_called = std::sync::Arc::new(std::sync::Mutex::new(false));
        let callback_called_clone = callback_called.clone();

        let callback = || async {
            *callback_called_clone.lock().unwrap() = true;
            Ok(())
        };

        // Act
        manager
            .invoke_if_different(marker_path.clone(), &marker_file, callback)
            .await
            .unwrap();

        // Assert
        assert!(*callback_called.lock().unwrap());
        assert!(marker_path.exists());

        let written_content = fs::read_to_string(marker_path).await.unwrap();
        let expected_content = serde_yaml::to_string(&marker_file).unwrap();
        assert_eq!(written_content, expected_content);
    }

    #[tokio::test]
    async fn test_invoke_if_different_does_not_call_callback_when_file_is_same() {
        // Arrange
        let temp_dir = tempdir().unwrap();
        let marker_path = temp_dir.path().join("marker.yaml");
        let marker_file = MarkerFile {
            sha256: "dummysha256".to_string(),
            url: "https://example.com/test.zip".to_string(),
        };
        let yaml_content = serde_yaml::to_string(&marker_file).unwrap();
        fs::write(&marker_path, yaml_content).await.unwrap();

        let manager = FileMarkerManager {};

        let callback_called = std::sync::Arc::new(std::sync::Mutex::new(false));
        let callback_called_clone = callback_called.clone();

        let callback = || async {
            *callback_called_clone.lock().unwrap() = true;
            Ok(())
        };

        // Act
        manager
            .invoke_if_different(marker_path.clone(), &marker_file, callback)
            .await
            .unwrap();

        // Assert
        assert!(!*callback_called.lock().unwrap());
    }

    #[tokio::test]
    async fn test_invoke_if_different_replaces_file_when_different() {
        // Arrange
        let temp_dir = tempdir().unwrap();
        let marker_path = temp_dir.path().join("marker.yaml");
        let old_marker_file = MarkerFile {
            sha256: "oldsha256".to_string(),
            url: "https://example.com/old.zip".to_string(),
        };
        let old_yaml_content = serde_yaml::to_string(&old_marker_file).unwrap();
        fs::write(&marker_path, old_yaml_content).await.unwrap();

        let new_marker_file = MarkerFile {
            sha256: "newsha256".to_string(),
            url: "https://example.com/new.zip".to_string(),
        };

        let manager = FileMarkerManager {};

        let callback_called = std::sync::Arc::new(std::sync::Mutex::new(false));
        let callback_called_clone = callback_called.clone();

        let callback = || async {
            *callback_called_clone.lock().unwrap() = true;
            Ok(())
        };

        // Act
        manager
            .invoke_if_different(marker_path.clone(), &new_marker_file, callback)
            .await
            .unwrap();

        // Assert
        assert!(*callback_called.lock().unwrap());
        let written_content = fs::read_to_string(marker_path).await.unwrap();
        let expected_content = serde_yaml::to_string(&new_marker_file).unwrap();
        assert_eq!(written_content, expected_content);
    }
}