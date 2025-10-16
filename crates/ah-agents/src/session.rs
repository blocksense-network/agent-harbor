/// Session archive export/import functionality
use crate::traits::{AgentError, AgentResult};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use std::fs::File;
use std::path::Path;
use tar::{Archive, Builder};
use tracing::{debug, info};

/// Export a directory to a compressed tar.gz archive
///
/// Creates an archive containing all files and subdirectories from `source_dir`.
/// The archive will be created at `dest_path` with .tar.gz extension.
pub async fn export_directory(source_dir: &Path, dest_path: &Path) -> AgentResult<()> {
    info!("Exporting session from {:?} to {:?}", source_dir, dest_path);

    if !source_dir.exists() {
        return Err(AgentError::SessionExportFailed(format!(
            "Source directory does not exist: {:?}",
            source_dir
        )));
    }

    // Create parent directory for archive if it doesn't exist
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Use blocking IO for tar operations
    let source_dir = source_dir.to_path_buf();
    let dest_path = dest_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = File::create(&dest_path)
            .map_err(|e| AgentError::SessionExportFailed(format!("Failed to create archive file: {}", e)))?;

        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(encoder);

        // Add all files from source directory
        tar.append_dir_all(".", &source_dir)
            .map_err(|e| AgentError::SessionExportFailed(format!("Failed to add files to archive: {}", e)))?;

        tar.finish()
            .map_err(|e| AgentError::SessionExportFailed(format!("Failed to finalize archive: {}", e)))?;

        debug!("Session export completed successfully");
        Ok(())
    })
    .await
    .map_err(|e| AgentError::SessionExportFailed(format!("Task join error: {}", e)))?
}

/// Import a compressed tar.gz archive to a directory
///
/// Extracts all files from the archive at `archive_path` into `dest_dir`.
/// The destination directory will be created if it doesn't exist.
pub async fn import_directory(
    archive_path: &Path,
    dest_dir: &Path,
) -> AgentResult<()> {
    info!("Importing session from {:?} to {:?}", archive_path, dest_dir);

    if !archive_path.exists() {
        return Err(AgentError::SessionImportFailed(format!(
            "Archive file does not exist: {:?}",
            archive_path
        )));
    }

    // Create destination directory
    tokio::fs::create_dir_all(dest_dir).await?;

    // Use blocking IO for tar operations
    let archive_path = archive_path.to_path_buf();
    let dest_dir = dest_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = File::open(&archive_path)
            .map_err(|e| AgentError::SessionImportFailed(format!("Failed to open archive: {}", e)))?;

        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        archive.unpack(&dest_dir)
            .map_err(|e| AgentError::SessionImportFailed(format!("Failed to extract archive: {}", e)))?;

        debug!("Session import completed successfully");
        Ok(())
    })
    .await
    .map_err(|e| AgentError::SessionImportFailed(format!("Task join error: {}", e)))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::{self, write};

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        let temp = TempDir::new().unwrap();

        // Create source directory with test files
        let source_dir = temp.path().join("source");
        fs::create_dir(&source_dir).unwrap();
        write(source_dir.join("file1.txt"), "content1").unwrap();
        write(source_dir.join("file2.txt"), "content2").unwrap();

        let subdir = source_dir.join("subdir");
        fs::create_dir(&subdir).unwrap();
        write(subdir.join("file3.txt"), "content3").unwrap();

        // Export to archive
        let archive_path = temp.path().join("session.tar.gz");
        export_directory(&source_dir, &archive_path).await.unwrap();
        assert!(archive_path.exists());

        // Import to new directory
        let dest_dir = temp.path().join("dest");
        let result = import_directory(&archive_path, &dest_dir).await;
        assert!(result.is_ok());

        // Verify files were restored
        assert!(dest_dir.join("file1.txt").exists());
        assert!(dest_dir.join("file2.txt").exists());
        assert!(dest_dir.join("subdir/file3.txt").exists());

        let content1 = fs::read_to_string(dest_dir.join("file1.txt")).unwrap();
        assert_eq!(content1, "content1");
    }
}
