//! Generic configuration engine with schema validation, merging, and provenance tracking.
//!
//! This crate provides the core functionality for loading, validating, merging, and
//! extracting configuration from various sources (files, environment variables, CLI flags).
//! All operations work on serde_json::Value for field-agnostic processing.

pub mod enforcement;
pub mod env;
pub mod extract;
pub mod loader;
pub mod merge;
pub mod paths;
pub mod provenance;
pub mod schema;

pub use provenance::{Scope, Scope::*};
pub use schema::SchemaRoot;

use anyhow::Result;
use serde_json::Value as J;

/// Final resolved configuration with provenance information
#[derive(Debug)]
pub struct Resolved {
    /// Final merged JSON configuration
    pub json: J,
    /// Provenance tracking for all configuration values
    pub provenance: provenance::Provenance,
}

/// Load and merge all configuration layers according to precedence rules
///
/// Precedence order: system < user < repo < repo-user < env < cli-config < flags
pub fn load_all(paths: &paths::Paths, flag_sets: &[(&str, &str)]) -> Result<Resolved> {
    use Scope::*;

    let mut prov = provenance::Provenance::default();
    let mut json = serde_json::json!({});

    // Load system layer (may contain enforcement rules)
    let system_layer = if paths.system.exists() {
        Some(loader::read_layer_from_file(&paths.system, System)?)
    } else {
        None
    };

    // Extract enforcement rules from system layer
    let enforcement = if let Some(ref layer) = system_layer {
        enforcement_from_layer(&layer.json)
    } else {
        enforcement::Enforcement::default()
    };

    // Load all layers, keeping them alive for the merge process
    let user_layer = paths
        .user
        .exists()
        .then(|| loader::read_layer_from_file(&paths.user, User).ok())
        .flatten();
    let repo_layer = paths
        .repo
        .as_ref()
        .and_then(|p| p.exists().then(|| loader::read_layer_from_file(p, Repo).ok()))
        .flatten();
    let repo_user_layer = paths
        .repo_user
        .as_ref()
        .and_then(|p| p.exists().then(|| loader::read_layer_from_file(p, RepoUser).ok()))
        .flatten();
    let env_layer = env::env_overlay()?;
    let cli_config_layer = paths
        .cli_config
        .as_ref()
        .and_then(|p| p.exists().then(|| loader::read_layer_from_file(p, CliConfig).ok()))
        .flatten();
    let flags_layer = env::flags_overlay(flag_sets);

    // Define layers in precedence order
    let layers = vec![
        (system_layer.as_ref().map(|l| &l.json), System),
        (user_layer.as_ref().map(|l| &l.json), User),
        (repo_layer.as_ref().map(|l| &l.json), Repo),
        (repo_user_layer.as_ref().map(|l| &l.json), RepoUser),
        (Some(&env_layer), Env),
        (cli_config_layer.as_ref().map(|l| &l.json), CliConfig),
        (Some(&flags_layer), Flags),
    ];

    // Merge layers in precedence order
    for (layer_opt, scope) in layers {
        if let Some(layer) = layer_opt {
            let mut masked_layer = layer.clone();

            // Mask enforced keys for non-system scopes
            if scope != System {
                enforcement::mask_layer(&mut masked_layer, &enforcement);
            }

            let before = json.clone();
            merge::merge_two_json(&mut json, masked_layer.clone());
            // Record provenance for the actual values that were set
            record_layer_provenance(&masked_layer, scope, &mut prov, "");
        }
    }

    // Mark enforced keys in provenance
    prov.enforced.extend(enforcement.keys.into_iter());

    Ok(Resolved {
        json,
        provenance: prov,
    })
}

/// Record provenance for all values in a layer
fn record_layer_provenance(
    layer: &J,
    scope: Scope,
    prov: &mut provenance::Provenance,
    prefix: &str,
) {
    use serde_json::Value::*;
    match layer {
        Object(obj) => {
            for (k, v) in obj {
                let pfx = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                record_layer_provenance(v, scope, prov, &pfx);
            }
        }
        Array(arr) => {
            // Record the entire array
            prov.winner.insert(prefix.to_string(), scope);
            prov.changes.entry(prefix.to_string()).or_default().push((scope, layer.clone()));
        }
        _ => {
            // Record scalar/null values
            prov.winner.insert(prefix.to_string(), scope);
            prov.changes.entry(prefix.to_string()).or_default().push((scope, layer.clone()));
        }
    }
}

/// Extract enforcement rules from system layer
fn enforcement_from_layer(system_json: &J) -> enforcement::Enforcement {
    let mut set = std::collections::BTreeSet::new();
    if let Some(arr) = system_json.get("enforced").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                set.insert(s.to_string());
            }
        }
    }
    enforcement::Enforcement { keys: set }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_toml_parsing() {
        let toml = r#"
            ui = "tui"
            [repo]
            task-runner = "just"
        "#;

        let json = loader::parse_toml_to_json(toml).unwrap();
        assert_eq!(json["ui"], "tui");
        assert_eq!(json["repo"]["task-runner"], "just");
    }

    #[test]
    fn test_schema_validation() {
        let valid_toml = r#"
            ui = "tui"
            [repo]
            supported-agents = "all"
            [repo.init]
            task-runner = "just"
        "#;

        let json = loader::parse_toml_to_json(valid_toml).unwrap();
        assert!(loader::validate_against_schema(&json).is_ok());
    }

    #[test]
    fn test_invalid_schema_validation() {
        let invalid_toml = r#"
            ui = "invalid-ui-value"
        "#;

        let json = loader::parse_toml_to_json(invalid_toml).unwrap();
        assert!(loader::validate_against_schema(&json).is_err());
    }

    #[test]
    fn test_merge_deep_objects() {
        let mut base = serde_json::json!({"a": {"b": 1}});
        let layer = serde_json::json!({"a": {"c": 2}});

        merge::merge_two_json(&mut base, layer);
        assert_eq!(base["a"]["b"], 1);
        assert_eq!(base["a"]["c"], 2);
    }

    #[test]
    fn test_merge_arrays_replace() {
        let mut base = serde_json::json!({"arr": [1, 2]});
        let layer = serde_json::json!({"arr": [3, 4]});

        merge::merge_two_json(&mut base, layer);
        assert_eq!(base["arr"], serde_json::json!([3, 4]));
    }

    #[test]
    fn test_insert_dotted() {
        let mut root = serde_json::json!({});
        merge::insert_dotted(&mut root, "repo.task-runner", serde_json::json!("just"));
        assert_eq!(root["repo"]["task-runner"], "just");
    }

    #[test]
    fn test_enforcement_masking() {
        let mut layer = serde_json::json!({"remote-server": "user-value", "ui": "tui"});
        let enforcement = enforcement::Enforcement {
            keys: ["remote-server".to_string()].into(),
        };

        enforcement::mask_layer(&mut layer, &enforcement);
        assert!(!layer.as_object().unwrap().contains_key("remote-server"));
        assert_eq!(layer["ui"], "tui");
    }

    #[test]
    fn test_provenance_tracking() {
        let before = serde_json::json!({"key": "old"});
        let after = serde_json::json!({"key": "new"});

        let mut prov = provenance::Provenance::default();
        provenance::record_diff(&before, &after, Scope::User, &mut prov, "");

        assert_eq!(prov.winner["key"], Scope::User);
        assert_eq!(
            prov.changes["key"][0],
            (Scope::User, serde_json::json!("new"))
        );
    }

    #[test]
    fn test_paths_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_root).unwrap();

        let paths = paths::discover_paths(Some(&repo_root));

        assert!(paths.system.to_string_lossy().contains("agent-harbor"));
        assert!(paths.user.to_string_lossy().contains("agent-harbor"));
        assert!(paths.repo.unwrap().starts_with(&repo_root));
        assert!(paths.repo_user.unwrap().starts_with(&repo_root));
    }

    #[test]
    fn test_env_overlay() {
        // Test that env overlay handles AH_ prefix correctly
        let overlay = env::env_overlay().unwrap();
        // Should be empty in test environment without AH_ vars
        assert!(overlay.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_flags_overlay() {
        let flags = vec![("repo.task-runner", "just"), ("ui", "tui")];
        let overlay = env::flags_overlay(&flags);

        assert_eq!(overlay["repo"]["task-runner"], "just");
        assert_eq!(overlay["ui"], "tui");
    }

    #[test]
    fn test_typed_extraction() {
        let json = serde_json::json!({
            "ui": "tui",
            "tui-font-style": "nerdfont",
            "repo": {
                "supported-agents": "all",
                "init": {
                    "task-runner": "just"
                }
            }
        });

        // UI fields are now flattened at root level
        let ui_root: ah_config_types::ui::UiRoot = extract::get(&json).unwrap();
        assert_eq!(ui_root.ui, Some(ah_config_types::ui::Ui::Tui));
        assert_eq!(ui_root.tui_font_style, Some("nerdfont".to_string()));

        let repo_config: ah_config_types::repo::RepoConfig =
            extract::get_at(&json, "repo").unwrap();
        assert_eq!(
            repo_config.supported_agents,
            Some(ah_config_types::repo::SupportedAgents::All)
        );
        assert_eq!(repo_config.init.task_runner, Some("just".to_string()));
    }

    #[test]
    fn test_load_all_integration() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_root).unwrap();

        // Create a temporary user config file
        let user_config_path = temp_dir.path().join("user_config.toml");
        std::fs::write(
            &user_config_path,
            r#"
            ui = "tui"
            [repo]
            supported-agents = "all"
        "#,
        )
        .unwrap();

        // Create a repo config file
        let repo_config_path = repo_root.join(".agents").join("config.toml");
        std::fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
        std::fs::write(
            &repo_config_path,
            r#"
            [repo.init]
            task-runner = "just"
        "#,
        )
        .unwrap();

        // Create paths manually for testing
        let paths = paths::Paths {
            system: temp_dir.path().join("system_config.toml"), // doesn't exist
            user: user_config_path,
            repo: Some(repo_config_path),
            repo_user: None,
            cli_config: None,
        };

        let resolved = load_all(&paths, &[]).unwrap();

        // Check that values were merged correctly
        assert_eq!(resolved.json["ui"], "tui"); // UI field is now flattened to root
        assert_eq!(resolved.json["repo"]["supported-agents"], "all");
        assert_eq!(resolved.json["repo"]["init"]["task-runner"], "just");

        // Check provenance
        println!("Provenance winners: {:?}", resolved.provenance.winner);
        assert_eq!(resolved.provenance.winner.get("ui"), Some(&Scope::User));
        assert_eq!(
            resolved.provenance.winner.get("repo.supported-agents"),
            Some(&Scope::User)
        );
        assert_eq!(
            resolved.provenance.winner.get("repo.init.task-runner"),
            Some(&Scope::Repo)
        );
    }
}
