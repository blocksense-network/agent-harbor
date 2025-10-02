import { Component } from "solid-js";
import {
  KeyboardShortcutsFooter,
  KeyboardShortcutsFooterProps,
} from "../common/KeyboardShortcutsFooter.js";

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
  const keyboardProps: KeyboardShortcutsFooterProps = {
    ...(props.onNewDraft && { onNewTask: props.onNewDraft }),
    ...(props.agentCount !== undefined && { agentCount: props.agentCount }),
    ...(props.focusState && { focusState: props.focusState }),
  };

  return <KeyboardShortcutsFooter {...keyboardProps} />;
};
