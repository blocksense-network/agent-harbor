/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import {
  Component,
  createResource,
  createEffect,
  createSignal,
  Show,
  For,
  onMount,
  onCleanup,
} from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { useBreadcrumbs } from '../../contexts/BreadcrumbContext';
import { apiClient } from '../../lib/api.js';

interface TaskDetailsPageProps {
  taskId?: string;
}

// Static mock data for milestone 4.6.1 - lexicographically sorted
const mockModifiedFiles = [
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

const mockAgentEvents = [
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
    lastLine: 'running 5 tests\n...\n5 passed, 0 failed',
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

// Function to get appropriate diff content for each file
const getDiffContentForFile = (filePath: string) => {
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
      return mockDiffContent; // fallback
  }
};

// Simple syntax highlighter for Rust code
const highlightSyntax = (code: string) => {
  if (!code || code.trim() === '') {
    return <span>{code || '\u00A0'}</span>;
  }

  // Keywords to highlight
  const keywords = [
    'fn',
    'let',
    'mut',
    'if',
    'else',
    'for',
    'while',
    'loop',
    'match',
    'return',
    'struct',
    'enum',
    'impl',
    'trait',
    'use',
    'mod',
    'pub',
    'crate',
    'super',
    'Self',
    'self',
    'true',
    'false',
    'Some',
    'None',
    'Ok',
    'Err',
    'Result',
    'Box',
    'Vec',
    'HashMap',
    'String',
    'println',
    'eprintln',
    'format',
    'std',
    'collections',
    'io',
    'error',
    'process',
    'env',
    'args',
    'collect',
    'exit',
    'path',
    'Path',
    'exists',
    'fs',
    'read_to_string',
    'dyn',
    'Error',
    'into',
    'as',
    'ref',
  ];

  const types = ['i32', 'u32', 'i64', 'u64', 'usize', 'isize', 'f32', 'f64', 'bool', 'char'];

  // Split code into tokens while preserving whitespace and indentation
  const tokens = code
    .split(/(\s+|[{}();,.=<>!+\-*/&|?:[\]]|\w+|"[^"]*"|'[^']*'|\/\/.*)/g)
    .filter(t => t !== undefined && t !== '');

  return (
    <span style={{ 'white-space': 'pre', 'tab-size': '4' }}>
      <For each={tokens}>
        {(token, _index) => {
          if (keywords.includes(token)) {
            return <span class="font-medium text-purple-600">{token}</span>;
          } else if (types.includes(token)) {
            return <span class="font-medium text-orange-600">{token}</span>;
          } else if (token.startsWith('"') && token.endsWith('"')) {
            return <span class="text-green-600">{token}</span>;
          } else if (token.startsWith("'") && token.endsWith("'")) {
            return <span class="text-green-600">{token}</span>;
          } else if (token.startsWith('//')) {
            return <span class="text-gray-500 italic">{token}</span>;
          } else if (/^[{}();,.=<>!+\-*/&|?:[\]]+$/.test(token)) {
            return <span class="text-blue-500">{token}</span>;
          } else {
            return <span>{token}</span>;
          }
        }}
      </For>
    </span>
  );
};

// Component to render syntax-highlighted diff
const DiffViewer: Component<{ content: string }> = props => {
  const parseDiff = (diffContent: string) => {
    const lines = diffContent.split('\n');
    const result: Array<{
      type: 'context' | 'addition' | 'deletion' | 'hunk';
      content: string;
      lineNumber?: number;
    }> = [];

    let leftLineNumber = 0;
    let rightLineNumber = 0;

    for (const line of lines) {
      if (line.startsWith('@@')) {
        // Hunk header
        result.push({ type: 'hunk', content: line });
      } else if (line.startsWith('+')) {
        rightLineNumber++;
        result.push({
          type: 'addition',
          content: line.substring(1),
          lineNumber: rightLineNumber,
        });
      } else if (line.startsWith('-')) {
        leftLineNumber++;
        result.push({
          type: 'deletion',
          content: line.substring(1),
          lineNumber: leftLineNumber,
        });
      } else if (line.startsWith(' ')) {
        leftLineNumber++;
        rightLineNumber++;
        result.push({
          type: 'context',
          content: line.substring(1),
          lineNumber: rightLineNumber,
        });
      } else {
        // Empty or other lines
        result.push({ type: 'context', content: line });
      }
    }

    return result;
  };

  const lines = () => parseDiff(props.content);

  return (
    <div class="font-mono text-sm leading-relaxed">
      <For each={lines()}>
        {(line, _index) => {
          const lineClasses = {
            hunk: 'bg-gray-100 text-gray-700 px-2 py-1 text-xs border-l-4 border-blue-400',
            addition: 'bg-green-50 text-green-800 border-l-4 border-green-400',
            deletion: 'bg-red-50 text-red-800 border-l-4 border-red-400',
            context: 'bg-gray-50 text-gray-700 border-l-4 border-gray-300',
          };

          return (
            <div
              class={`
                flex
                ${lineClasses[line.type]}
              `}
            >
              <div class="w-12 pr-2 text-right text-gray-500 select-none">
                {line.lineNumber || ''}
              </div>
              <div class="flex-1 pl-2">
                {line.type === 'hunk' ? (
                  <span class="font-bold">{line.content}</span>
                ) : (
                  highlightSyntax(line.content || '\u00A0')
                )}
              </div>
            </div>
          );
        }}
      </For>
    </div>
  );
};

export const TaskDetailsPage: Component<TaskDetailsPageProps> = props => {
  const params = useParams();
  const navigate = useNavigate();
  const { setBreadcrumbs } = useBreadcrumbs();
  const taskId = () => props.taskId || params['id'];

  // Search and filter state
  const [searchQuery, setSearchQuery] = createSignal('');
  const [statusFilter, setStatusFilter] = createSignal<string>('all');

  // Load task details from API
  const [taskData] = createResource(taskId, async id => {
    if (!id) return null;
    try {
      const result = await apiClient.getSession(id);
      return result;
    } catch (error) {
      console.error('Failed to load task details:', error);
      return null;
    }
  });

  const task = () => taskData();

  // Set breadcrumbs
  createEffect(() => {
    const currentTaskId = taskId();
    const currentTask = task();

    if (currentTaskId && currentTask) {
      setBreadcrumbs([
        {
          label: 'workspace',
          onClick: () => navigate('/'),
        },
        {
          label: `session-${currentTaskId}`,
        },
        {
          label: `Task ${currentTaskId}`,
        },
      ]);
    } else {
      setBreadcrumbs([]);
    }
  });

  // Clear breadcrumbs on unmount
  onCleanup(() => {
    setBreadcrumbs([]);
  });

  const handleFileClick = (filePath: string) => {
    const anchorId = filePath.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();
    const element = document.getElementById(anchorId);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  };

  const handleNavigateToFile = (direction: 'prev' | 'next', currentIndex: number) => {
    const filteredFiles = () => filteredModifiedFiles();
    const targetIndex = direction === 'prev' ? currentIndex - 1 : currentIndex + 1;
    if (targetIndex >= 0 && targetIndex < filteredFiles().length) {
      const targetFile = filteredFiles()[targetIndex];
      if (targetFile) {
        handleFileClick(targetFile.path);
      }
    }
  };

  // Filter files based on search query and status
  const filteredModifiedFiles = () => {
    return mockModifiedFiles.filter(file => {
      const matchesSearch =
        searchQuery() === '' || file.path.toLowerCase().includes(searchQuery().toLowerCase());
      const matchesStatus = statusFilter() === 'all' || file.status === statusFilter();
      return matchesSearch && matchesStatus;
    });
  };

  const getStatusBadge = (status: string) => {
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

  onMount(() => {
    if (typeof window === 'undefined') return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        navigate('/');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    onCleanup(() => window.removeEventListener('keydown', handleKeyDown));
  });

  return (
    <Show
      when={task()}
      fallback={
        <div class="flex min-h-screen items-center justify-center bg-gray-50">
          <div class="text-center">
            <h2 class="mb-2 text-xl font-semibold text-gray-900">Task not found</h2>
            <p class="text-gray-600">The requested task could not be loaded.</p>
          </div>
        </div>
      }
    >
      <div class="min-h-screen bg-gray-50" data-testid="task-details">
        {/* Two-Panel Layout */}
        <div class="flex h-[calc(100vh-80px)]">
          {/* Left Panel (30% width) */}
          <div class="flex w-3/10 flex-col border-r border-gray-200 bg-white">
            {/* Modified Files Panel (top 40% of left panel) */}
            <div class="h-2/5 border-b border-gray-200 p-4">
              <h3 class="mb-3 text-sm font-semibold text-gray-900">Modified Files</h3>

              {/* Search and Filter Controls */}
              <div class="mb-3 space-y-2">
                <input
                  type="text"
                  placeholder="Search files..."
                  class={`
                    w-full rounded-md border border-gray-300 px-3 py-1 text-sm
                    focus:border-transparent focus:ring-2 focus:ring-blue-500
                    focus:outline-none
                  `}
                  value={searchQuery()}
                  onInput={e => setSearchQuery(e.currentTarget.value)}
                />
                <div class="flex space-x-1">
                  <button
                    class={`
                      rounded-md px-2 py-1 text-xs transition-colors
                      ${
                        statusFilter() === 'all'
                          ? 'border border-blue-200 bg-blue-100 text-blue-800'
                          : `
                            bg-gray-100 text-gray-600
                            hover:bg-gray-200
                          `
                      }
                    `}
                    onClick={() => setStatusFilter('all')}
                  >
                    All
                  </button>
                  <button
                    class={`
                      rounded-md px-2 py-1 text-xs transition-colors
                      ${
                        statusFilter() === 'modified'
                          ? 'border border-blue-200 bg-blue-100 text-blue-800'
                          : `
                            bg-gray-100 text-gray-600
                            hover:bg-gray-200
                          `
                      }
                    `}
                    onClick={() => setStatusFilter('modified')}
                  >
                    Modified
                  </button>
                  <button
                    class={`
                      rounded-md px-2 py-1 text-xs transition-colors
                      ${
                        statusFilter() === 'added'
                          ? 'border border-blue-200 bg-blue-100 text-blue-800'
                          : `
                            bg-gray-100 text-gray-600
                            hover:bg-gray-200
                          `
                      }
                    `}
                    onClick={() => setStatusFilter('added')}
                  >
                    Added
                  </button>
                  <button
                    class={`
                      rounded-md px-2 py-1 text-xs transition-colors
                      ${
                        statusFilter() === 'deleted'
                          ? 'border border-blue-200 bg-blue-100 text-blue-800'
                          : `
                            bg-gray-100 text-gray-600
                            hover:bg-gray-200
                          `
                      }
                    `}
                    onClick={() => setStatusFilter('deleted')}
                  >
                    Deleted
                  </button>
                </div>
              </div>

              <div class="max-h-48 space-y-2 overflow-y-auto">
                <For each={filteredModifiedFiles()}>
                  {(file, _index) => {
                    const badge = getStatusBadge(file.status);
                    return (
                      <div
                        class={`
                          flex cursor-pointer items-center justify-between
                          rounded p-2
                          hover:bg-gray-50
                        `}
                        onClick={() => handleFileClick(file.path)}
                      >
                        <div class="flex min-w-0 flex-1 items-center space-x-2">
                          <span
                            class={`
                              inline-flex items-center rounded-full border
                              px-1.5 py-1 text-xs font-medium
                              ${badge.bg}
                              ${badge.text}
                              ${badge.border}
                            `}
                          >
                            {badge.icon}
                          </span>
                          <span class="truncate text-sm text-gray-900" title={file.path}>
                            {file.path}
                          </span>
                        </div>
                        <div class="flex-shrink-0 text-xs text-gray-500">
                          +{file.linesAdded} -{file.linesRemoved}
                        </div>
                      </div>
                    );
                  }}
                </For>
                <Show when={filteredModifiedFiles().length === 0}>
                  <div class="py-4 text-center text-sm text-gray-500">
                    No files match the current filters
                  </div>
                </Show>
              </div>
            </div>

            {/* Agent Activity Panel (middle 60% of left panel) */}
            <div class="flex h-3/5 flex-col p-4">
              <h3 class="mb-3 text-sm font-semibold text-gray-900">Agent Activity</h3>
              <div class="flex-1 space-y-2 overflow-y-auto">
                <For each={mockAgentEvents}>
                  {event => (
                    <div class="flex space-x-3">
                      <span class="w-16 flex-shrink-0 text-xs text-gray-500">
                        {event.timestamp}
                      </span>
                      <div class="min-w-0 flex-1">
                        {event.type === 'thinking' && (
                          <div class="text-sm text-gray-700">
                            <span class="font-medium">💭 </span>
                            {event.content}
                          </div>
                        )}
                        {event.type === 'tool' && (
                          <div class="text-sm text-gray-700">
                            <span class="font-medium">🔧 </span>
                            {event.content}
                            {event.lastLine && (
                              <div
                                class={`
                                  mt-1 pl-4 font-mono text-xs text-gray-600
                                `}
                              >
                                {event.lastLine}
                              </div>
                            )}
                          </div>
                        )}
                        {event.type === 'file_edit' && (
                          <div class="text-sm text-gray-700">
                            <span class="font-medium">📝 </span>
                            {event.content}
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </div>

            {/* Chat Box (bottom 20% of left panel) */}
            <div class="h-1/5 border-t border-gray-200 p-3">
              <div class="flex h-full flex-col">
                {/* Message Composer */}
                <div class="flex flex-1 flex-col">
                  {/* Message Input with Integrated Controls */}
                  <div class="relative flex-1">
                    {/* Toolbar above input */}
                    <div class="mb-1 flex items-center justify-between px-1">
                      <div class="flex items-center space-x-1">
                        <button
                          class={`
                            rounded p-1 text-sm text-gray-500
                            hover:bg-gray-100 hover:text-gray-700
                          `}
                          title="Add file context"
                        >
                          📁
                        </button>
                        <button
                          class={`
                            rounded p-1 text-sm text-gray-500
                            hover:bg-gray-100 hover:text-gray-700
                          `}
                          title="Configure tools"
                        >
                          🔧
                        </button>
                        <button
                          class={`
                            rounded p-1 text-sm text-gray-500
                            hover:bg-gray-100 hover:text-gray-700
                          `}
                          title="Add attachments"
                        >
                          📎
                        </button>
                      </div>

                      <div class="flex items-center space-x-2">
                        {/* Subtle context window indicator */}
                        <div
                          class={`
                            flex items-center space-x-1 text-xs text-gray-400
                          `}
                        >
                          <div
                            class="h-2 w-2 rounded-full bg-green-400"
                            title="Context: 2.3K tokens | TPS: 45 | Cost: $0.02"
                          />
                          <span
                            class={`
                              hidden
                              sm:inline
                            `}
                          >
                            2.3K
                          </span>
                        </div>

                        <select
                          class={`
                            border-0 bg-transparent text-xs text-gray-500
                            focus:text-gray-700 focus:outline-none
                          `}
                        >
                          <option>GPT-4</option>
                          <option>Claude-3</option>
                          <option>Gemini Pro</option>
                        </select>
                      </div>
                    </div>

                    {/* Message Input with Send Button */}
                    <div class="flex">
                      <textarea
                        class={`
                          flex-1 resize-none rounded-l border border-gray-300
                          px-3 py-2 text-sm
                          focus:border-transparent focus:ring-2
                          focus:ring-blue-500 focus:outline-none
                        `}
                        placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
                        rows="2"
                      />
                      <button
                        class={`
                          rounded-r bg-blue-600 px-4 py-2 text-white
                          hover:bg-blue-700
                          focus:ring-2 focus:ring-blue-500 focus:outline-none
                        `}
                      >
                        Send
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Right Panel (70% width) - Unified Diff View */}
          <div class="w-7/10 bg-white">
            <div class="h-full overflow-y-auto">
              <For each={filteredModifiedFiles()}>
                {(file, index) => {
                  const anchorId = file.path.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();
                  return (
                    <div
                      id={anchorId}
                      class={`
                        border-b border-gray-200
                        last:border-b-0
                      `}
                    >
                      {/* File Header */}
                      <div
                        class={`
                          sticky top-0 z-10 border-b border-gray-300 bg-white
                          p-4
                        `}
                      >
                        <div class="flex items-center justify-between">
                          <div class="flex items-center space-x-3">
                            {(() => {
                              const badge = getStatusBadge(file.status);
                              return (
                                <span
                                  class={`
                                    inline-flex items-center rounded-full border
                                    px-2 py-1 text-xs font-medium
                                    ${badge.bg}
                                    ${badge.text}
                                    ${badge.border}
                                  `}
                                >
                                  {badge.icon} {badge.label}
                                </span>
                              );
                            })()}
                            <h3
                              class={`
                                font-mono text-lg font-semibold text-gray-900
                              `}
                            >
                              {file.path}
                            </h3>
                            <span class="text-sm text-gray-600">
                              +{file.linesAdded} -{file.linesRemoved} lines
                            </span>
                          </div>
                          <div class="flex space-x-2">
                            <button
                              class={`
                                rounded bg-gray-100 px-3 py-1 text-sm
                                hover:bg-gray-200
                              `}
                            >
                              Load Full File
                            </button>
                            <button
                              class={`
                                rounded bg-gray-100 px-3 py-1 text-sm
                                hover:bg-gray-200
                                disabled:cursor-not-allowed disabled:opacity-50
                              `}
                              disabled={index() === 0}
                              onClick={() => handleNavigateToFile('prev', index())}
                            >
                              Previous
                            </button>
                            <button
                              class={`
                                rounded bg-gray-100 px-3 py-1 text-sm
                                hover:bg-gray-200
                                disabled:cursor-not-allowed disabled:opacity-50
                              `}
                              disabled={index() === mockModifiedFiles.length - 1}
                              onClick={() => handleNavigateToFile('next', index())}
                            >
                              Next
                            </button>
                          </div>
                        </div>
                      </div>

                      {/* Diff Content */}
                      <div class="p-4">
                        <div
                          class={`
                            overflow-x-auto rounded-lg border border-gray-200
                            bg-white
                          `}
                        >
                          <DiffViewer content={getDiffContentForFile(file.path)} />
                        </div>
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};
