import { Component, createSignal, onMount, onCleanup, Show } from "solid-js";

export type FooterContext = 
  | "task-feed" 
  | "draft-task" 
  | "modal"
  | "default";

interface KeyboardShortcutsFooterProps {
  onNewTask?: () => void;
  agentCount?: number;
  focusState?: {
    focusedElement: 'draft-textarea' | 'session-card' | 'none';
    focusedDraftId?: string;
    focusedSessionId?: string;
  };
}

export const KeyboardShortcutsFooter: Component<KeyboardShortcutsFooterProps> = (props) => {
  const [isMac, setIsMac] = createSignal(false);

  // Detect platform for keyboard shortcut display
  onMount(() => {
    if (typeof window !== 'undefined') {
      setIsMac(navigator.platform.toUpperCase().indexOf('MAC') >= 0);
    }
  });

  // Determine context based on focus state
  const getContext = (): FooterContext => {
    if (!props.focusState) return "default";
    
    switch (props.focusState.focusedElement) {
      case 'draft-textarea':
        return "draft-task";
      case 'session-card':
        return "task-feed";
      default:
        return "default";
    }
  };

  // Get dynamic shortcut text based on focus state
  const getEnterShortcut = () => {
    const context = getContext();
    
    switch (context) {
      case "draft-task":
        const agentCount = props.agentCount || 0;
        return agentCount > 1 ? "Launch Agents" : "Launch Agent";
      case "task-feed":
        return "Review Session Details";
      default:
        return "Go";
    }
  };
  
  const handleNewTask = () => {
    props.onNewTask?.();
  };

  const modKey = () => isMac() ? "Cmd" : "Ctrl";
  const agentText = () => {
    const count = props.agentCount || 0;
    return count === 1 ? "Agent" : "Agents";
  };

  return (
    <footer
      class="border-t border-gray-200 bg-white px-4 py-2 flex items-center justify-between text-sm"
      role="contentinfo"
      aria-label="Keyboard shortcuts"
    >
      {/* New Task button on the left */}
      <div class="flex items-center" role="toolbar" aria-label="Actions">
        <Show when={props.onNewTask}>
          <button
            onClick={props.onNewTask}
            class="flex items-center gap-1 px-3 py-1 bg-blue-600 text-white rounded text-xs hover:bg-blue-700 transition-colors cursor-pointer border border-blue-700"
            aria-label={`New draft task (${modKey()}+N)`}
          >
            <kbd class="font-semibold">{modKey()}+N</kbd>
            <span>New Draft Task</span>
          </button>
        </Show>
      </div>

      {/* Context-sensitive shortcuts on the right */}
      <div class="flex items-center gap-2" role="toolbar" aria-label="Keyboard shortcuts">
        <Show when={getContext() === "task-feed"}>
          {/* Informational shortcuts (non-clickable but styled like buttons) */}
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">↑↓</kbd>
            <span>Navigate</span>
          </div>
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
        </Show>

        <Show when={getContext() === "draft-task"}>
          {/* Informational shortcuts for draft context */}
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">Shift+Enter</kbd>
            <span>New Line</span>
          </div>
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">Tab</kbd>
            <span>Next Field</span>
          </div>
        </Show>

        <Show when={getContext() === "default"}>
          {/* Default context shortcuts */}
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">↑↓</kbd>
            <span>Navigate</span>
          </div>
          <div class="flex items-center gap-1 px-2 py-1 bg-gray-100 rounded text-xs text-gray-700 border border-gray-200">
            <kbd class="font-semibold">Enter</kbd>
            <span>{getEnterShortcut()}</span>
          </div>
        </Show>
      </div>
    </footer>
  );
};
