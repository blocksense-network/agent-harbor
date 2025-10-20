//! Tool profiles for different coding agents
//!
//! This module implements the tool profile system equivalent to the Python server.py
//! tools_mapping, providing agent-specific tool mappings and validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported coding agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentType {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "opencode")]
    Opencode,
    #[serde(rename = "qwen")]
    Qwen,
    #[serde(rename = "cursor-cli")]
    CursorCli,
    #[serde(rename = "goose")]
    Goose,
}

/// Tool mapping entry for a specific scenario tool event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMapping {
    /// Agent-specific tool name
    pub name: String,
    /// Whether this uses direct mapping (vs template-based)
    pub direct: bool,
    /// Argument mapping from scenario event args to agent tool args
    pub args_map: HashMap<String, String>,
    /// Template args for non-direct mappings
    pub template_args: Option<HashMap<String, serde_yaml::Value>>,
}

/// Tool profiles for all supported agents
#[derive(Debug)]
pub struct ToolProfiles {
    /// Valid tool names for each agent
    pub valid_tools: HashMap<AgentType, std::collections::HashSet<String>>,
    /// Tool mappings from scenario event types to agent-specific implementations
    pub tool_mappings: HashMap<AgentType, HashMap<String, ToolMapping>>,
}

impl Default for ToolProfiles {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolProfiles {
    /// Create tool profiles with all supported agent mappings
    pub fn new() -> Self {
        let mut profiles = Self {
            valid_tools: HashMap::new(),
            tool_mappings: HashMap::new(),
        };

        profiles.initialize_codex_tools();
        profiles.initialize_claude_tools();
        profiles.initialize_other_agents();

        profiles
    }

    /// Get valid tools for an agent
    pub fn get_valid_tools(&self, agent: AgentType) -> Option<&std::collections::HashSet<String>> {
        self.valid_tools.get(&agent)
    }

    /// Get tool mapping for an agent and scenario event type
    pub fn get_tool_mapping(
        &self,
        agent: AgentType,
        scenario_event_type: &str,
    ) -> Option<&ToolMapping> {
        self.tool_mappings.get(&agent)?.get(scenario_event_type)
    }

    /// Validate if a tool name is valid for the given agent
    pub fn is_valid_tool(&self, agent: AgentType, tool_name: &str) -> bool {
        self.valid_tools
            .get(&agent)
            .map(|tools| tools.contains(tool_name))
            .unwrap_or(false)
    }

    /// Get valid tools for an agent (returns empty set if agent not found)
    pub fn valid_tools_for_agent_type(
        &self,
        agent: AgentType,
    ) -> std::collections::HashSet<String> {
        self.valid_tools.get(&agent).cloned().unwrap_or_default()
    }

    /// Map scenario tool event to agent-specific tool call
    pub fn map_tool_call(
        &self,
        agent: AgentType,
        scenario_event_type: &str,
        scenario_args: &HashMap<String, serde_yaml::Value>,
    ) -> Option<super::ToolCall> {
        let mapping = self.get_tool_mapping(agent, scenario_event_type)?;

        if mapping.direct {
            // Direct mapping - use the mapped tool name and remap arguments
            let mut mapped_args = HashMap::new();
            for (scenario_key, agent_key) in &mapping.args_map {
                if let Some(value) = scenario_args.get(scenario_key) {
                    mapped_args.insert(agent_key.clone(), value.clone());
                }
            }

            // Include any unmapped arguments
            for (key, value) in scenario_args {
                if !mapping.args_map.contains_key(key) {
                    mapped_args.insert(key.clone(), value.clone());
                }
            }

            Some(super::ToolCall {
                id: format!("call_{}", uuid::Uuid::new_v4()),
                name: mapping.name.clone(),
                args: mapped_args,
            })
        } else {
            // Template-based mapping (typically run_terminal_cmd with command templates)
            let template_args = mapping.template_args.as_ref()?;

            // Substitute scenario args into the template
            let mut mapped_args = HashMap::new();
            for (key, value) in template_args {
                if let serde_yaml::Value::String(template) = value {
                    // String template substitution
                    let substituted = self.substitute_template(template, scenario_args);
                    mapped_args.insert(key.clone(), serde_yaml::Value::String(substituted));
                } else {
                    mapped_args.insert(key.clone(), value.clone());
                }
            }

            // Merge any additional scenario args that weren't templated
            for (key, value) in scenario_args {
                if !mapped_args.contains_key(key) {
                    mapped_args.insert(key.clone(), value.clone());
                }
            }

            Some(super::ToolCall {
                id: format!("call_{}", uuid::Uuid::new_v4()),
                name: mapping.name.clone(),
                args: mapped_args,
            })
        }
    }

    /// Substitute template variables with scenario args
    fn substitute_template(
        &self,
        template: &str,
        args: &HashMap<String, serde_yaml::Value>,
    ) -> String {
        let mut result = template.to_string();
        for (key, value) in args {
            let placeholder = format!("{{{}}}", key);
            if let serde_yaml::Value::String(value_str) = value {
                result = result.replace(&placeholder, value_str);
            } else {
                // For non-string values, convert to string
                result = result.replace(
                    &placeholder,
                    &serde_json::to_string(&value).unwrap_or_default(),
                );
            }
        }
        result
    }

    /// Initialize Codex tool mappings
    fn initialize_codex_tools(&mut self) {
        let mut codex_tools = std::collections::HashSet::new();
        codex_tools.insert("write_file".to_string());
        codex_tools.insert("read_file".to_string());
        codex_tools.insert("run_command".to_string());
        codex_tools.insert("append_file".to_string());
        codex_tools.insert("replace_in_file".to_string());

        self.valid_tools.insert(AgentType::Codex, codex_tools);

        let mut codex_mappings = HashMap::new();

        // Canonical tool names from Scenario Format
        codex_mappings.insert(
            "writeFile".to_string(),
            ToolMapping {
                name: "write_file".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "path".to_string()),
                    ("content".to_string(), "text".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "readFile".to_string(),
            ToolMapping {
                name: "read_file".to_string(),
                direct: true,
                args_map: [("path".to_string(), "path".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "runCmd".to_string(),
            ToolMapping {
                name: "run_command".to_string(),
                direct: true,
                args_map: [
                    ("cmd".to_string(), "command".to_string()),
                    ("cwd".to_string(), "cwd".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        // Backward compatibility for old playbook tool names (snake_case)
        codex_mappings.insert(
            "write_file".to_string(),
            ToolMapping {
                name: "write_file".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "path".to_string()),
                    ("text".to_string(), "text".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "read_file".to_string(),
            ToolMapping {
                name: "read_file".to_string(),
                direct: true,
                args_map: [("path".to_string(), "path".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "run_command".to_string(),
            ToolMapping {
                name: "run_command".to_string(),
                direct: true,
                args_map: [
                    ("command".to_string(), "command".to_string()),
                    ("cwd".to_string(), "cwd".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "append_file".to_string(),
            ToolMapping {
                name: "append_file".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "path".to_string()),
                    ("text".to_string(), "text".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        codex_mappings.insert(
            "replace_in_file".to_string(),
            ToolMapping {
                name: "replace_in_file".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "path".to_string()),
                    ("old".to_string(), "old".to_string()),
                    ("new".to_string(), "new".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        self.tool_mappings.insert(AgentType::Codex, codex_mappings);
    }

    /// Initialize Claude tool mappings (comprehensive)
    fn initialize_claude_tools(&mut self) {
        let mut claude_tools = std::collections::HashSet::new();
        // Updated to match actual Claude 2.0.5 tool definitions
        claude_tools.insert("Bash".to_string());
        claude_tools.insert("Grep".to_string());
        claude_tools.insert("Read".to_string());
        claude_tools.insert("Glob".to_string());
        claude_tools.insert("Edit".to_string());
        claude_tools.insert("Write".to_string());
        claude_tools.insert("Task".to_string());
        claude_tools.insert("WebFetch".to_string());
        claude_tools.insert("WebSearch".to_string());
        claude_tools.insert("TodoWrite".to_string());
        claude_tools.insert("NotebookEdit".to_string());
        claude_tools.insert("ExitPlanMode".to_string());
        claude_tools.insert("BashOutput".to_string());
        claude_tools.insert("KillShell".to_string());
        claude_tools.insert("SlashCommand".to_string());

        self.valid_tools.insert(AgentType::Claude, claude_tools);

        let mut claude_mappings = HashMap::new();

        // Canonical tool names from Scenario Format
        claude_mappings.insert(
            "runCmd".to_string(),
            ToolMapping {
                name: "Bash".to_string(),
                direct: true,
                args_map: [
                    ("cmd".to_string(), "command".to_string()),
                    ("cwd".to_string(), "cwd".to_string()),
                    ("timeout".to_string(), "timeout".to_string()),
                    ("description".to_string(), "description".to_string()),
                    (
                        "run_in_background".to_string(),
                        "run_in_background".to_string(),
                    ),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        // Legacy scenario tool names (for backward compatibility with existing scenarios)
        claude_mappings.insert(
            "writeFile".to_string(),
            ToolMapping {
                name: "Write".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("text".to_string(), "content".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "appendFile".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("text".to_string(), "text".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "replaceInFile".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("old_str".to_string(), "old_string".to_string()),
                    ("new_str".to_string(), "new_string".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        // Direct mappings for Claude tool names (for scenarios that use them directly)
        claude_mappings.insert(
            "Write".to_string(),
            ToolMapping {
                name: "Write".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("text".to_string(), "content".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "Edit".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("old_string".to_string(), "old_string".to_string()),
                    ("new_string".to_string(), "new_string".to_string()),
                    ("text".to_string(), "old_string".to_string()), // For append operations
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "grep".to_string(),
            ToolMapping {
                name: "Grep".to_string(),
                direct: true,
                args_map: [
                    ("pattern".to_string(), "pattern".to_string()),
                    ("path".to_string(), "path".to_string()),
                    ("glob".to_string(), "glob".to_string()),
                    ("output_mode".to_string(), "output_mode".to_string()),
                    ("-B".to_string(), "-B".to_string()),
                    ("-A".to_string(), "-A".to_string()),
                    ("-C".to_string(), "-C".to_string()),
                    ("-n".to_string(), "-n".to_string()),
                    ("-i".to_string(), "-i".to_string()),
                    ("type".to_string(), "type".to_string()),
                    ("head_limit".to_string(), "head_limit".to_string()),
                    ("multiline".to_string(), "multiline".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "readFile".to_string(),
            ToolMapping {
                name: "Read".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("offset".to_string(), "offset".to_string()),
                    ("limit".to_string(), "limit".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "writeFile".to_string(),
            ToolMapping {
                name: "Write".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("text".to_string(), "content".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "editFile".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("old_string".to_string(), "old_string".to_string()),
                    ("new_string".to_string(), "new_string".to_string()),
                    ("replace_all".to_string(), "replace_all".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        // Claude doesn't have native append, so map to Edit
        claude_mappings.insert(
            "appendFile".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [("path".to_string(), "file_path".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "replaceInFile".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("old_string".to_string(), "old_string".to_string()),
                    ("new_string".to_string(), "new_string".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "listDir".to_string(),
            ToolMapping {
                name: "Glob".to_string(),
                direct: true,
                args_map: [
                    ("pattern".to_string(), "pattern".to_string()),
                    ("path".to_string(), "path".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "task".to_string(),
            ToolMapping {
                name: "Task".to_string(),
                direct: true,
                args_map: [
                    ("description".to_string(), "description".to_string()),
                    ("prompt".to_string(), "prompt".to_string()),
                    ("subagent_type".to_string(), "subagent_type".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "webFetch".to_string(),
            ToolMapping {
                name: "WebFetch".to_string(),
                direct: true,
                args_map: [
                    ("url".to_string(), "url".to_string()),
                    ("prompt".to_string(), "prompt".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "webSearch".to_string(),
            ToolMapping {
                name: "WebSearch".to_string(),
                direct: true,
                args_map: [
                    ("query".to_string(), "query".to_string()),
                    ("allowed_domains".to_string(), "allowed_domains".to_string()),
                    ("blocked_domains".to_string(), "blocked_domains".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "todoWrite".to_string(),
            ToolMapping {
                name: "TodoWrite".to_string(),
                direct: true,
                args_map: [("todos".to_string(), "todos".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "notebookEdit".to_string(),
            ToolMapping {
                name: "NotebookEdit".to_string(),
                direct: true,
                args_map: [
                    ("notebook_path".to_string(), "notebook_path".to_string()),
                    ("cell_id".to_string(), "cell_id".to_string()),
                    ("new_source".to_string(), "new_source".to_string()),
                    ("cell_type".to_string(), "cell_type".to_string()),
                    ("edit_mode".to_string(), "edit_mode".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "exitPlanMode".to_string(),
            ToolMapping {
                name: "ExitPlanMode".to_string(),
                direct: true,
                args_map: [("plan".to_string(), "plan".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "bashOutput".to_string(),
            ToolMapping {
                name: "BashOutput".to_string(),
                direct: true,
                args_map: [
                    ("bash_id".to_string(), "bash_id".to_string()),
                    ("filter".to_string(), "filter".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "killShell".to_string(),
            ToolMapping {
                name: "KillShell".to_string(),
                direct: true,
                args_map: [("shell_id".to_string(), "shell_id".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "slashCommand".to_string(),
            ToolMapping {
                name: "SlashCommand".to_string(),
                direct: true,
                args_map: [("command".to_string(), "command".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        // Backward compatibility mappings for old playbook tool names (snake_case)
        claude_mappings.insert(
            "write_file".to_string(),
            ToolMapping {
                name: "Write".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("text".to_string(), "content".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "read_file".to_string(),
            ToolMapping {
                name: "Read".to_string(),
                direct: true,
                args_map: [("path".to_string(), "file_path".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "append_file".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [("path".to_string(), "file_path".to_string())].into_iter().collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "replace_in_file".to_string(),
            ToolMapping {
                name: "Edit".to_string(),
                direct: true,
                args_map: [
                    ("path".to_string(), "file_path".to_string()),
                    ("old_string".to_string(), "old".to_string()),
                    ("new_string".to_string(), "new".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        claude_mappings.insert(
            "run_command".to_string(),
            ToolMapping {
                name: "Bash".to_string(),
                direct: true,
                args_map: [
                    ("command".to_string(), "command".to_string()),
                    ("cwd".to_string(), "cwd".to_string()),
                ]
                .into_iter()
                .collect(),
                template_args: None,
            },
        );

        self.tool_mappings.insert(AgentType::Claude, claude_mappings);
    }

    /// Initialize other agents (placeholder implementations)
    fn initialize_other_agents(&mut self) {
        // Gemini - placeholder
        self.valid_tools.insert(AgentType::Gemini, std::collections::HashSet::new());
        self.tool_mappings.insert(AgentType::Gemini, HashMap::new());

        // OpenCode - placeholder
        self.valid_tools.insert(AgentType::Opencode, std::collections::HashSet::new());
        self.tool_mappings.insert(AgentType::Opencode, HashMap::new());

        // Qwen - placeholder
        self.valid_tools.insert(AgentType::Qwen, std::collections::HashSet::new());
        self.tool_mappings.insert(AgentType::Qwen, HashMap::new());

        // Cursor CLI - placeholder
        self.valid_tools.insert(AgentType::CursorCli, std::collections::HashSet::new());
        self.tool_mappings.insert(AgentType::CursorCli, HashMap::new());

        // Goose - placeholder
        self.valid_tools.insert(AgentType::Goose, std::collections::HashSet::new());
        self.tool_mappings.insert(AgentType::Goose, HashMap::new());
    }
}
