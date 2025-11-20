// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Common types and abstractions for health check reporting

use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Comprehensive health status for an agent
#[derive(Debug, Clone)]
pub struct AgentHealthStatus {
    /// Agent name (e.g., "Cursor CLI", "Gemini CLI")
    pub name: String,
    /// Whether the agent binary/tool is available
    pub available: bool,
    /// Version information if available
    pub version: Option<String>,
    /// Whether the agent is properly authenticated
    pub authenticated: bool,
    /// Authentication method used (e.g., "API Key", "OAuth", "Session Token")
    pub auth_method: Option<String>,
    /// Authentication source/details (may contain sensitive info)
    pub auth_source: Option<String>,
    /// Any errors encountered during status check
    pub error: Option<String>,
    /// Whether the status check timed out
    pub timed_out: bool,
    /// Additional notes or warnings
    pub notes: Vec<String>,
    /// Agent-specific metadata
    pub metadata: HashMap<String, String>,
}

impl AgentHealthStatus {
    /// Create a new health status with the given agent name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            available: false,
            version: None,
            authenticated: false,
            auth_method: None,
            auth_source: None,
            error: None,
            timed_out: false,
            notes: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Mark the agent as available with optional version
    pub fn with_availability(mut self, available: bool, version: Option<String>) -> Self {
        self.available = available;
        self.version = version;
        self
    }

    /// Set authentication status and details
    pub fn with_auth(
        mut self,
        authenticated: bool,
        auth_method: Option<String>,
        auth_source: Option<String>,
    ) -> Self {
        self.authenticated = authenticated;
        self.auth_method = auth_method;
        self.auth_source = auth_source;
        self
    }

    /// Add an error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Mark as timed out
    #[allow(dead_code)]
    pub fn with_timeout(mut self) -> Self {
        self.timed_out = true;
        self.error = Some("Status check timed out".to_string());
        self
    }

    /// Add a note/warning
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add metadata
    #[allow(dead_code)]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if the agent has any issues (unavailable, unauthenticated, or errors)
    #[allow(dead_code)]
    pub fn has_issues(&self) -> bool {
        !self.available || !self.authenticated || self.error.is_some() || self.timed_out
    }

    /// Get a summary status (OK, Warning, Error)
    pub fn status_level(&self) -> HealthStatusLevel {
        if !self.available || self.error.is_some() || self.timed_out {
            HealthStatusLevel::Error
        } else if !self.authenticated || !self.notes.is_empty() {
            HealthStatusLevel::Warning
        } else {
            HealthStatusLevel::Ok
        }
    }
}

/// Overall health status level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatusLevel {
    Ok,
    Warning,
    Error,
}

impl HealthStatusLevel {
    #[allow(dead_code)]
    pub fn emoji(&self) -> &'static str {
        match self {
            HealthStatusLevel::Ok => "✅",
            HealthStatusLevel::Warning => "⚠️",
            HealthStatusLevel::Error => "❌",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatusLevel::Ok => "ok",
            HealthStatusLevel::Warning => "warning",
            HealthStatusLevel::Error => "error",
        }
    }
}

/// Trait for formatting health status in different output formats
pub trait HealthFormatter {
    /// Format a single agent's health status
    fn format_agent_status(&self, status: &AgentHealthStatus, with_credentials: bool) -> String;

    /// Format multiple agents' health status
    fn format_agents_summary(&self, agents: &[AgentHealthStatus], with_credentials: bool)
    -> String;

    /// Format terminal environment information
    fn format_terminal_info(
        &self,
        environments: &[String],
        multiplexers: &HashMap<String, String>,
    ) -> String;
}

/// Human-readable formatter for console output
pub struct HumanReadableFormatter;

impl HumanReadableFormatter {
    /// Get Kitty configuration guidance based on current setup
    fn get_kitty_configuration_guidance(&self) -> String {
        use ah_mux::KittyMultiplexer;

        let mut guidance = String::new();
        guidance.push_str("💡 Kitty Configuration Check:\n");

        // Try to create a KittyMultiplexer and check configuration
        match KittyMultiplexer::default().check_configuration() {
            Ok(()) => {
                guidance.push_str("   ✅ Kitty is properly configured for Agent Harbor\n");
                guidance.push_str("   ✅ Remote control is enabled\n");
                guidance.push_str("   ✅ Socket listening is configured\n");
                guidance.push_str("   ✅ Layout splitting is enabled\n");
            }
            Err(error) => {
                guidance.push_str("   ⚠️  Kitty configuration issues detected:\n");

                // Parse the error message to provide specific guidance
                let error_str = error.to_string();
                if error_str.contains("allow_remote_control") {
                    guidance.push_str("   ❌ Remote control is not enabled\n");
                    guidance.push_str("   📝 Add to ~/.config/kitty/kitty.conf:\n");
                    guidance.push_str("      allow_remote_control yes\n");
                }
                if error_str.contains("listen_on") {
                    guidance.push_str("   ❌ Socket listening is not configured\n");
                    guidance.push_str("   📝 Add to ~/.config/kitty/kitty.conf:\n");
                    guidance.push_str("      listen_on unix:/tmp/kitty-ah.sock\n");
                }
                if error_str.contains("enabled_layouts") {
                    guidance.push_str("   ❌ Layout splitting is not enabled\n");
                    guidance.push_str("   📝 Add to ~/.config/kitty/kitty.conf:\n");
                    guidance.push_str("      enabled_layouts splits\n");
                }
                if error_str.contains("not installed") || error_str.contains("not in PATH") {
                    guidance.push_str("   ❌ Kitty is not installed or not in PATH\n");
                    guidance.push_str("   📝 Install Kitty: https://sw.kovidgoyal.net/kitty/\n");
                }

                guidance.push_str(
                    "   🔄 After making changes, restart Kitty or reload config (Ctrl+Shift+F5)\n",
                );
            }
        }

        guidance
    }
}

impl HealthFormatter for HumanReadableFormatter {
    fn format_agent_status(&self, status: &AgentHealthStatus, with_credentials: bool) -> String {
        let mut output = String::new();
        output.push_str(&format!("{}:\n", status.name));

        // Availability and version
        if status.available {
            if let Some(version) = &status.version {
                output.push_str(&format!("  ✅ Available (v{})\n", version));
            } else {
                output.push_str("  ✅ Available\n");
            }
        } else {
            if let Some(error) = &status.error {
                output.push_str(&format!("  ❌ {}\n", error));
            } else {
                output.push_str("  ❌ Not available\n");
            }
            return output;
        }

        // Authentication status
        if status.authenticated {
            if let Some(auth_method) = &status.auth_method {
                output.push_str(&format!("  ✅ Authenticated via {}\n", auth_method));
            } else {
                output.push_str("  ✅ Authenticated\n");
            }

            if let Some(auth_source) = &status.auth_source {
                if with_credentials {
                    output.push_str(&format!("  ✅ Authentication source: {}\n", auth_source));
                } else {
                    output.push_str("  ✅ Authentication source found (use --with-credentials to display details)\n");
                }
            }
        } else {
            output.push_str("  ⚠️  Not authenticated");
            if let Some(error) = &status.error {
                output.push_str(&format!(" - {}", error));
            }
            output.push('\n');
        }

        // Notes and warnings
        for note in &status.notes {
            output.push_str(&format!("  💡 {}\n", note));
        }

        // Timeout indicator
        if status.timed_out {
            output.push_str("  ❌ Status check timed out\n");
        }

        output
    }

    fn format_agents_summary(
        &self,
        agents: &[AgentHealthStatus],
        with_credentials: bool,
    ) -> String {
        let mut output = String::new();
        output.push_str(&format!("\n{:-<40}\n", ""));
        output.push_str("🤖 Agent Health\n");
        output.push_str(&format!("{:-<40}\n", ""));

        for agent in agents {
            output.push_str(&self.format_agent_status(agent, with_credentials));
            output.push('\n');
        }

        output
    }

    fn format_terminal_info(
        &self,
        environments: &[String],
        multiplexers: &HashMap<String, String>,
    ) -> String {
        let mut output = String::new();

        // Terminal environments
        output.push_str(&format!("{:-<40}\n", ""));
        output.push_str("📺 Terminal Environment\n");
        output.push_str(&format!("{:-<40}\n", ""));

        if environments.is_empty() {
            output.push_str("❌ No terminal environment detected\n");
        } else {
            output.push_str("✅ Detected environments (outermost to innermost):\n");
            for (i, env) in environments.iter().enumerate() {
                let indent = "  ".repeat(i);
                output.push_str(&format!("{}{}\n", indent, env));
            }

            // Check for Kitty-specific configuration guidance
            if environments.iter().any(|env| env.contains("Kitty")) {
                output.push('\n');
                output.push_str(&self.get_kitty_configuration_guidance());
            }
        }
        output.push('\n');

        // Multiplexer availability
        output.push_str(&format!("{:-<40}\n", ""));
        output.push_str("🔧 Terminal Multiplexer Availability\n");
        output.push_str(&format!("{:-<40}\n", ""));

        for (name, status) in multiplexers {
            let (emoji, desc) = match status.as_str() {
                "available" => ("✅", "Available"),
                "not_available" => ("❌", "Not available"),
                "failed_to_initialize" => ("❌", "Failed to initialize"),
                _ => ("❓", "Unknown status"),
            };
            output.push_str(&format!("{} {}: {}\n", emoji, name, desc));
        }

        output.push('\n');
        output
    }
}

/// JSON formatter for structured output
pub struct JsonFormatter;

impl JsonFormatter {
    pub fn format_full_report(
        &self,
        environments: &[String],
        multiplexers: &HashMap<String, String>,
        agents: &[AgentHealthStatus],
        with_credentials: bool,
    ) -> JsonValue {
        let mut report = serde_json::Map::new();

        // Add each agent directly at the top level as per spec
        for agent in agents {
            let agent_key = agent.name.to_lowercase().replace(" ", "_");
            report.insert(agent_key, self.format_agent_json(agent, with_credentials));
        }

        // Also include terminal environment info (not specified in spec but useful)
        report.insert(
            "terminal_environment".to_string(),
            serde_json::json!({
                "detected_environments": environments,
                "multiplexer_availability": multiplexers
            }),
        );

        JsonValue::Object(report)
    }

    fn format_agent_json(&self, status: &AgentHealthStatus, with_credentials: bool) -> JsonValue {
        let mut agent_json = serde_json::json!({
            "present": status.available,
            "authenticated": status.authenticated,
        });

        if let Some(version) = &status.version {
            agent_json["version"] = JsonValue::String(version.clone());
        }

        // Create details object for additional information
        let mut details = serde_json::Map::new();
        details.insert(
            "status_level".to_string(),
            JsonValue::String(status.status_level().as_str().to_string()),
        );

        if let Some(auth_method) = &status.auth_method {
            details.insert(
                "auth_method".to_string(),
                JsonValue::String(auth_method.clone()),
            );
        }

        if let Some(auth_source) = &status.auth_source {
            if with_credentials {
                // When --with-credentials is used, include the actual token/source as per spec
                agent_json["token"] = JsonValue::String(auth_source.clone());
                details.insert(
                    "auth_source".to_string(),
                    JsonValue::String(auth_source.clone()),
                );
            } else {
                details.insert("auth_source_available".to_string(), JsonValue::Bool(true));
            }
        }

        if let Some(error) = &status.error {
            details.insert("error".to_string(), JsonValue::String(error.clone()));
        }

        if status.timed_out {
            details.insert("timed_out".to_string(), JsonValue::Bool(true));
        }

        if !status.notes.is_empty() {
            details.insert(
                "notes".to_string(),
                JsonValue::Array(
                    status.notes.iter().map(|note| JsonValue::String(note.clone())).collect(),
                ),
            );
        }

        if !status.metadata.is_empty() {
            let metadata_json: serde_json::Map<String, JsonValue> = status
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), JsonValue::String(v.clone())))
                .collect();
            details.insert("metadata".to_string(), JsonValue::Object(metadata_json));
        }

        agent_json["details"] = JsonValue::Object(details);
        agent_json
    }
}
