import { Component } from "solid-js";
import { KeyboardShortcutsFooter } from "../common/KeyboardShortcutsFooter.js";

export interface FooterProps {
  onNewDraft?: () => void;
  agentCount?: number;
  focusState?: {
    focusedElement: "draft-textarea" | "session-card" | "none";
    focusedDraftId?: string;
    focusedSessionId?: string;
  };
}

export const Footer: Component<FooterProps> = (props) => {
  const keyboardProps: any = {};
  if (props.onNewDraft) keyboardProps.onNewTask = props.onNewDraft;
  if (props.agentCount !== undefined)
    keyboardProps.agentCount = props.agentCount;
  if (props.focusState) keyboardProps.focusState = props.focusState;

  return <KeyboardShortcutsFooter {...keyboardProps} />;
};
