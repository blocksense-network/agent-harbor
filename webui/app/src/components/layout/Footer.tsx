import { Component, createMemo } from "solid-js";
import {
  KeyboardShortcutsFooter,
  type KeyboardShortcutsFooterProps,
} from "../common/KeyboardShortcutsFooter.js";

export interface FooterProps {
  onNewDraft?: () => void;
  agentCount?: number | undefined;
  focusState?: {
    focusedElement: "draft-textarea" | "session-card" | "none";
    focusedDraftId?: string;
    focusedSessionId?: string;
    focusedDraftAgentCount?: number;
  };
}

export const Footer: Component<FooterProps> = (props) => {
  const resolvedAgentCount = createMemo(
    () => props.agentCount ?? props.focusState?.focusedDraftAgentCount,
  );

  const computedProps = createMemo<Partial<KeyboardShortcutsFooterProps>>(
    () => {
      const partial: Partial<KeyboardShortcutsFooterProps> = {};

      if (props.onNewDraft) {
        partial.onNewTask = props.onNewDraft;
      }

      const agentCount = resolvedAgentCount();
      if (agentCount !== undefined) {
        partial.agentCount = agentCount;
      }

      if (props.focusState) {
        partial.focusState = props.focusState;
      }

      return partial;
    },
  );

  return <KeyboardShortcutsFooter {...computedProps()} />;
};
