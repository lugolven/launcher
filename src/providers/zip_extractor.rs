use std::{io::Read, os::unix::fs::PermissionsExt};

use tokio::io::AsyncWriteExt;

pub struct ZipExtractor {}

impl crate::providers::extractor::Extractor for ZipExtractor {
    fn new() -> impl crate::providers::extractor::Extractor {
        ZipExtractor {}
    }

    async fn extract_to_disk(
        &self,
        content: &bytes::Bytes,
        folder: &std::path::PathBuf,
        executable_path: &std::path::PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(content.to_vec())).or_else(|e| {
                Err::<_, Box<dyn std::error::Error>>(
                    format!("Failed to read zip archive: {}", e).into(),
                )
            })?;
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
}


#[cfg(test)]
mod tests {
    use crate::providers::extractor::Extractor;

    use super::*;
    use bytes::Bytes;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_extract_to_disk() {
        // Arrange
        let temp_dir = tempdir().unwrap();
        let folder = temp_dir.path().to_path_buf();
        let executable_path = folder.join("test_executable");

        // Create a mock ZIP file in memory
        let mut zip_buffer = Vec::new();
        {
            let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buffer));
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            zip_writer
                .start_file("test_file.txt", options)
                .unwrap();
            zip_writer
                .write_all(b"Hello, world!")
                .unwrap();
            zip_writer
                .start_file("test_executable", options)
                .unwrap();
            zip_writer
                .write_all(b"Executable content")
                .unwrap();
            zip_writer.finish().unwrap();
        }

        let content = Bytes::from(zip_buffer);

        // Act
        let extractor = ZipExtractor::new();
        extractor
            .extract_to_disk(&content, &folder, &executable_path)
            .await
            .unwrap();

        // Assert
        let extracted_file_path = folder.join("test_file.txt");
        let extracted_executable_path = folder.join("test_executable");

        assert!(extracted_file_path.exists());
        assert!(extracted_executable_path.exists());

        let file_content = fs::read_to_string(extracted_file_path).unwrap();
        assert_eq!(file_content, "Hello, world!");

        let executable_content = fs::read_to_string(&extracted_executable_path).unwrap();
        assert_eq!(executable_content, "Executable content");

        let metadata = fs::metadata(&extracted_executable_path).unwrap();
        assert!(metadata.permissions().mode() & 0o755 != 0);
    }
}


