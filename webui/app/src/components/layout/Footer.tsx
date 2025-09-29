import { Component } from "solid-js";
import { KeyboardShortcutsFooter } from "../common/KeyboardShortcutsFooter.js";

export interface FooterProps {
  onNewDraft?: () => void;
  agentCount?: number;
  focusState?: {
    focusedElement: 'draft-textarea' | 'session-card' | 'none';
    focusedDraftId?: string;
    focusedSessionId?: string;
  };
}

export const Footer: Component<FooterProps> = (props) => {
  return (
    <KeyboardShortcutsFooter 
      onNewTask={props.onNewDraft}
      agentCount={props.agentCount || 0}
      focusState={props.focusState}
    />
  );
};