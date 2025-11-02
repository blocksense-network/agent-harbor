// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Cross-platform credential management for AI agents
use crate::traits::{AgentError, AgentResult};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

/// Copy files from source to destination, preserving directory structure
///
/// This is a helper function for copying credential files and configurations
/// from the user's home directory to a custom agent HOME directory.
pub async fn copy_files(files: &[PathBuf], src_base: &Path, dst_base: &Path) -> AgentResult<()> {
    for file in files {
        let src_path = src_base.join(file);
        let dst_path = dst_base.join(file);

        if src_path.exists() {
            // Create parent directory if needed
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    AgentError::CredentialCopyFailed(format!(
                        "Failed to create directory {:?}: {}",
                        parent, e
                    ))
                })?;
            }

            // Copy the file
            fs::copy(&src_path, &dst_path).await.map_err(|e| {
                AgentError::CredentialCopyFailed(format!(
                    "Failed to copy {:?} to {:?}: {}",
                    src_path, dst_path, e
                ))
            })?;

            debug!("Copied {:?} to {:?}", src_path, dst_path);
        } else {
            warn!("Credential file {:?} does not exist, skipping", src_path);
        }
    }

    Ok(())
}

/// Copy an entire directory recursively
pub async fn copy_directory(src: &Path, dst: &Path) -> AgentResult<()> {
    copy_directory_impl(src, dst).await
}

/// Internal recursive implementation
fn copy_directory_impl<'a>(
    src: &'a Path,
    dst: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AgentResult<()>> + Send + 'a>> {
    Box::pin(async move {
        if !src.exists() {
            warn!("Source directory {:?} does not exist, skipping", src);
            return Ok(());
        }

        fs::create_dir_all(dst).await.map_err(|e| {
            AgentError::CredentialCopyFailed(format!(
                "Failed to create destination directory {:?}: {}",
                dst, e
            ))
        })?;

        let mut entries = fs::read_dir(src).await.map_err(|e| {
            AgentError::CredentialCopyFailed(format!("Failed to read directory {:?}: {}", src, e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentError::CredentialCopyFailed(format!("Failed to read directory entry: {}", e))
        })? {
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);

            let metadata = entry.metadata().await.map_err(|e| {
                AgentError::CredentialCopyFailed(format!(
                    "Failed to read metadata for {:?}: {}",
                    src_path, e
                ))
            })?;

            if metadata.is_dir() {
                copy_directory_impl(&src_path, &dst_path).await?;
            } else {
                fs::copy(&src_path, &dst_path).await.map_err(|e| {
                    AgentError::CredentialCopyFailed(format!(
                        "Failed to copy {:?} to {:?}: {}",
                        src_path, dst_path, e
                    ))
                })?;
                debug!("Copied {:?} to {:?}", src_path, dst_path);
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    #[ah_test_utils::logged_tokio_test]
    async fn test_copy_files() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(src.join(".claude")).await.unwrap();
        write(src.join(".claude/config.json"), "{}").unwrap();

        let files = vec![PathBuf::from(".claude/config.json")];
        let result = copy_files(&files, &src, &dst).await;

        assert!(result.is_ok());
        assert!(dst.join(".claude/config.json").exists());
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_copy_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).await.unwrap();
        write(src.join("file1.txt"), "content").unwrap();

        let subdir = src.join("subdir");
        fs::create_dir_all(&subdir).await.unwrap();
        write(subdir.join("file2.txt"), "content2").unwrap();

        let result = copy_directory(&src, &dst).await;

        assert!(result.is_ok());
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());
    }
}
