use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Extract devshell names from a flake.nix file
///
/// This function replicates the logic from the Ruby `devshell_names` method.
/// It first tries to use `nix eval` to properly parse the flake for the current system,
/// then falls back to parsing all devShells regardless of system, and finally
/// uses regex parsing as a last resort for malformed flakes.
pub async fn devshell_names(root: &Path) -> Result<Vec<String>> {
    let flake_path = root.join("flake.nix");
    if !flake_path.exists() {
        return Ok(Vec::new());
    }

    // First try using nix eval to properly parse the flake for current system
    if let Ok(shells) = devshell_names_for_current_system(&flake_path).await {
        return Ok(shells);
    }

    // Fallback: try to get all devShells regardless of system
    if let Ok(shells) = devshell_names_all_systems(&flake_path).await {
        return Ok(shells);
    }

    // Final fallback to regex parsing for malformed flakes (e.g., in tests)
    devshell_names_regex(&flake_path).await
}

/// Try to get devshell names for the current system using nix eval
async fn devshell_names_for_current_system(flake_path: &Path) -> Result<Vec<String>> {
    // Get the current system first
    let system_output = Command::new("nix")
        .args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .context("Failed to run nix eval for current system")?;

    if !system_output.status.success() {
        return Err(anyhow::anyhow!("nix eval failed for current system"));
    }

    let current_system = String::from_utf8(system_output.stdout)
        .context("Invalid UTF-8 in nix eval output")?
        .trim()
        .to_string();

    // Evaluate the devShells attribute for the current system
    let flake_ref = format!("{}#devShells.{}", flake_path.display(), current_system);
    let output = Command::new("nix")
        .args([
            "eval",
            "--json",
            "--no-warn-dirty",
            &flake_ref,
            "--apply",
            "builtins.attrNames",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .context("Failed to run nix eval for devShells")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("nix eval failed for devShells"));
    }

    let json_str =
        String::from_utf8(output.stdout).context("Invalid UTF-8 in nix eval devShells output")?;

    let names: Vec<String> =
        serde_json::from_str(&json_str).context("Failed to parse JSON output from nix eval")?;

    Ok(names)
}

/// Try to get devshell names for all systems using nix eval
async fn devshell_names_all_systems(flake_path: &Path) -> Result<Vec<String>> {
    // Nix expression to get devshell names from any system
    let nix_expr = r#"devShells: let systems = builtins.attrNames devShells;
in if systems == [] then []
else builtins.attrNames (devShells.${builtins.head systems})"#;

    let flake_ref = format!("{}#devShells", flake_path.display());
    let output = Command::new("nix")
        .args([
            "eval",
            "--json",
            "--no-warn-dirty",
            &flake_ref,
            "--apply",
            nix_expr,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .context("Failed to run nix eval for devShells (all systems)")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "nix eval failed for devShells (all systems)"
        ));
    }

    let json_str = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in nix eval devShells output (all systems)")?;

    let names: Vec<String> = serde_json::from_str(&json_str)
        .context("Failed to parse JSON output from nix eval (all systems)")?;

    Ok(names)
}

/// Final fallback: parse devshell names using regex
async fn devshell_names_regex(flake_path: &Path) -> Result<Vec<String>> {
    let content = tokio::fs::read_to_string(flake_path)
        .await
        .context("Failed to read flake.nix file")?;

    // Regex to match devShells.<system>.<name> = patterns
    let re = Regex::new(r"devShells\.[^.]+\.([A-Za-z0-9._-]+)\s*=")
        .context("Failed to create regex for devshell parsing")?;

    let mut names = Vec::new();
    for cap in re.captures_iter(&content) {
        if let Some(name) = cap.get(1) {
            let name_str = name.as_str().to_string();
            if !names.contains(&name_str) {
                names.push(name_str);
            }
        }
    }

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_devshell_names_no_flake() {
        let temp_dir = TempDir::new().unwrap();
        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert_eq!(names, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_devshell_names_regex_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let flake_content = r#"
        {
          outputs = { self }: {
            devShells.x86_64-linux.default = pkgs.mkShell {};
            devShells.x86_64-linux.custom = pkgs.mkShell {};
            devShells.aarch64-linux.default = pkgs.mkShell {};
          };
        }
        "#;
        fs::write(temp_dir.path().join("flake.nix"), flake_content).unwrap();

        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"default".to_string()));
        assert!(names.contains(&"custom".to_string()));
    }

    #[tokio::test]
    async fn test_devshell_names_empty_flake() {
        let temp_dir = TempDir::new().unwrap();
        let flake_content = r#"
        {
          outputs = { self }: {
            devShells.x86_64-linux = {};
          };
        }
        "#;
        fs::write(temp_dir.path().join("flake.nix"), flake_content).unwrap();

        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert_eq!(names, Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_devshell_validation_success() {
        let temp_dir = TempDir::new().unwrap();
        let flake_content = r#"
        {
          outputs = { self }: {
            devShells.x86_64-linux.default = pkgs.mkShell {};
            devShells.x86_64-linux.custom = pkgs.mkShell {};
          };
        }
        "#;
        fs::write(temp_dir.path().join("flake.nix"), flake_content).unwrap();

        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert!(names.contains(&"custom".to_string()));
        assert!(names.contains(&"default".to_string()));
    }

    #[tokio::test]
    async fn test_devshell_validation_failure() {
        let temp_dir = TempDir::new().unwrap();
        let flake_content = r#"
        {
          outputs = { self }: {
            devShells.x86_64-linux.default = pkgs.mkShell {};
          };
        }
        "#;
        fs::write(temp_dir.path().join("flake.nix"), flake_content).unwrap();

        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert!(!names.contains(&"missing".to_string()));
        assert!(names.contains(&"default".to_string()));
    }

    #[tokio::test]
    async fn test_devshell_without_flake() {
        let temp_dir = TempDir::new().unwrap();
        // No flake.nix file

        let names = devshell_names(temp_dir.path()).await.unwrap();
        assert_eq!(names, Vec::<String>::new());
    }
}
