/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export type ModifiedFile = {
  path: string;
  status: string;
  linesAdded: number;
  linesRemoved: number;
};

// Static mock data for milestone 4.6.1 - lexicographically sorted
export const mockModifiedFiles: ModifiedFile[] = [
  { path: 'Cargo.toml', status: 'modified', linesAdded: 2, linesRemoved: 1 },
  {
    path: 'src/config.rs',
    status: 'modified',
    linesAdded: 8,
    linesRemoved: 12,
  },
  { path: 'src/error.rs', status: 'deleted', linesAdded: 0, linesRemoved: 25 },
  { path: 'src/lib.rs', status: 'modified', linesAdded: 9, linesRemoved: 2 },
  { path: 'src/main.rs', status: 'modified', linesAdded: 6, linesRemoved: 2 },
  { path: 'src/utils.rs', status: 'modified', linesAdded: 7, linesRemoved: 2 },
  {
    path: 'tests/integration_test.rs',
    status: 'added',
    linesAdded: 67,
    linesRemoved: 0,
  },
];

export const mockAgentEvents = [
  {
    timestamp: '10:23:15',
    type: 'thinking',
    content: 'Analyzing the codebase structure and understanding the project requirements',
  },
  {
    timestamp: '10:23:45',
    type: 'tool',
    content: 'Running cargo check to validate current code',
    lastLine: 'Finished dev [unoptimized + debuginfo] target(s) in 2.34s',
  },
  {
    timestamp: '10:24:12',
    type: 'file_edit',
    content: 'Modified src/main.rs (+5 -2)',
  },
  {
    timestamp: '10:24:30',
    type: 'thinking',
    content: 'Considering how to implement the new feature based on the existing patterns',
  },
  {
    timestamp: '10:25:01',
    type: 'tool',
    content: 'Running tests to ensure no regressions',
    lastLine: 'running 5 tests\\n...\\n5 passed, 0 failed',
  },
  {
    timestamp: '10:25:15',
    type: 'file_edit',
    content: 'Modified src/lib.rs (+12 -0)',
  },
  {
    timestamp: '10:25:45',
    type: 'tool',
    content: 'Formatting code with rustfmt',
    lastLine: 'Format successful',
  },
];

// Enhanced diff content for main.rs - shorter file, showing most of it
const mockDiffContent = `@@ -1,8 +1,12 @@
 use std::collections::HashMap;

 fn main() {
-    println!("Hello, world!");
+    println!("Hello, Agent Harbor!");

+    // Initialize the agent system
+    let mut agents = HashMap::new();
+    agents.insert("coder", "AI coding assistant");
+    agents.insert("reviewer", "Code review specialist");

-    let x = 5;
+    let x = 42;
     println!("x = {}", x);
 }`;

// Longer diff content for lib.rs - showing only changed hunks with context
const libRsDiffContent = `@@ -15,7 +15,7 @@ pub struct AgentSystem {
     agents: HashMap<String, Box<dyn Agent>>,
     config: SystemConfig,
 }

-/// Initialize a new agent system with default configuration
+/// Initialize a new agent system with enhanced configuration
 impl AgentSystem {
     pub fn new() -> Result<Self, AgentError> {
         let config = SystemConfig::default();
@@ -45,12 +45,16 @@ impl AgentSystem {
     pub fn add_agent(&mut self, name: String, agent: Box<dyn Agent>) -> Result<(), AgentError> {
         if self.agents.contains_key(&name) {
             return Err(AgentError::AgentExists(name));
         }

+        // Validate agent capabilities before adding
+        if !agent.validate_capabilities()? {
+            return Err(AgentError::InvalidCapabilities);
+        }
+
         self.agents.insert(name, agent);
         Ok(())
     }

     pub fn get_agent(&self, name: &str) -> Option<&Box<dyn Agent>> {
@@ -78,8 +82,12 @@ impl AgentSystem {
         for (name, agent) in &self.agents {
             println!("Agent: {}", name);
             agent.status()?;
         }

+        // Log system health metrics
+        println!("Total agents: {}", self.agents.len());
+        println!("System uptime: {}ms", self.uptime_ms);
+
         Ok(())
     }
 }`;

// Diff content for deleted error.rs file
const errorRsDiffContent = `@@ -1,25 +0,0 @@
-use std::error::Error;
-use std::fmt;
-
-#[derive(Debug)]
-pub enum AgentError {
-    NotFound(String),
-    InvalidConfig(String),
-    ConnectionFailed(String),
-    Timeout,
-    PermissionDenied,
-}
-
-impl fmt::Display for AgentError {
-    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
-        match self {
-            AgentError::NotFound(name) => write!(f, "Agent '{}' not found", name),
-            AgentError::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
-            AgentError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
-            AgentError::Timeout => write!(f, "Operation timed out"),
-            AgentError::PermissionDenied => write!(f, "Permission denied"),
-        }
-    }
-}
-
-impl Error for AgentError {}`;

// Even longer diff content for utils.rs - showing only changed sections
const utilsRsDiffContent = `@@ -23,7 +23,8 @@ pub fn validate_agent_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

-    if name.len() > 50 {
+    // Increased limit to accommodate longer descriptive names
+    if name.len() > 100 {
         return Err(ValidationError::TooLong);
     }

@@ -67,15 +68,19 @@ pub fn parse_agent_config(config_str: &str) -> Result<AgentConfig, ConfigError> {
     let mut config = AgentConfig::default();

     for line in config_str.lines() {
         let line = line.trim();
         if line.is_empty() || line.starts_with('#') {
             continue;
         }

-        if let Some((key, value)) = line.split_once('=') {
+        // Support both '=' and ':' separators for flexibility
+        let separator = if line.contains('=') { '=' } else { ':' };
+        if let Some((key, value)) = line.split_once(separator) {
             let key = key.trim();
             let value = value.trim();

             match key {
                 "max_tokens" => config.max_tokens = value.parse()?,
+                "temperature" => config.temperature = value.parse()?,
                 "model" => config.model = value.to_string(),
                 "timeout_ms" => config.timeout_ms = value.parse()?,
                 _ => return Err(ConfigError::UnknownKey(key.to_string())),
             }
@@ -134,12 +139,16 @@ pub fn create_agent_task(
     let task = AgentTask {
         id: generate_task_id(),
         agent_name: agent_name.to_string(),
         prompt: prompt.to_string(),
         priority,
         created_at: std::time::SystemTime::now(),
         config: config.clone(),
     };

+    // Log task creation for monitoring
+    log::info!("Created task {} for agent {}", task.id, agent_name);
+
 Ok(task)
}`;

export const getDiffContentForFile = (filePath: string) => {
  switch (filePath) {
    case 'src/main.rs':
      return mockDiffContent;
    case 'src/lib.rs':
      return libRsDiffContent;
    case 'src/utils.rs':
      return utilsRsDiffContent;
    case 'src/error.rs':
      return errorRsDiffContent;
    default:
      return mockDiffContent;
  }
};

export const getStatusBadge = (status: string) => {
  switch (status) {
    case 'added':
      return {
        bg: 'bg-green-100',
        text: 'text-green-800',
        border: 'border-green-200',
        icon: '●',
        label: 'Added',
      };
    case 'modified':
      return {
        bg: 'bg-blue-100',
        text: 'text-blue-800',
        border: 'border-blue-200',
        icon: '○',
        label: 'Modified',
      };
    case 'deleted':
      return {
        bg: 'bg-red-100',
        text: 'text-red-800',
        border: 'border-red-200',
        icon: '✕',
        label: 'Deleted',
      };
    case 'renamed':
      return {
        bg: 'bg-yellow-100',
        text: 'text-yellow-800',
        border: 'border-yellow-200',
        icon: '→',
        label: 'Renamed',
      };
    default:
      return {
        bg: 'bg-gray-100',
        text: 'text-gray-800',
        border: 'border-gray-200',
        icon: '?',
        label: 'Unknown',
      };
  }
};
