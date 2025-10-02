import {
  createContext,
  useContext,
  Component,
  JSX,
  createSignal,
} from "solid-js";

interface FocusState {
  focusedElement: "draft-textarea" | "session-card" | "none";
  focusedDraftId?: string;
  focusedSessionId?: string;
}

interface FocusContextValue {
  focusState: () => FocusState;
  setDraftFocus: (draftId: string) => void;
  setSessionFocus: (sessionId: string) => void;
  clearFocus: () => void;
  isDraftFocused: (draftId: string) => boolean;
  isSessionFocused: (sessionId: string) => boolean;
}

const FocusContext = createContext<FocusContextValue>();

export const FocusProvider: Component<{ children: JSX.Element }> = (props) => {
  const [focusState, setFocusState] = createSignal<FocusState>({
    focusedElement: "none",
  });

  const setDraftFocus = (draftId: string) => {
    setFocusState({
      focusedElement: "draft-textarea",
      focusedDraftId: draftId,
    });
  };

  const setSessionFocus = (sessionId: string) => {
    setFocusState({
      focusedElement: "session-card",
      focusedSessionId: sessionId,
    });
  };

  const clearFocus = () => {
    setFocusState({
      focusedElement: "none",
    });
  };

  const isDraftFocused = (draftId: string) => {
    const state = focusState();
    return (
      state.focusedElement === "draft-textarea" &&
      state.focusedDraftId === draftId
    );
  };

  const isSessionFocused = (sessionId: string) => {
    const state = focusState();
    return (
      state.focusedElement === "session-card" &&
      state.focusedSessionId === sessionId
    );
  };

  const contextValue: FocusContextValue = {
    focusState,
    setDraftFocus,
    setSessionFocus,
    clearFocus,
    isDraftFocused,
    isSessionFocused,
  };

  return (
    <FocusContext.Provider value={contextValue}>
      {props.children}
    </FocusContext.Provider>
  );
};

export const useFocus = (): FocusContextValue => {
  const context = useContext(FocusContext);
  if (!context) {
    throw new Error("useFocus must be used within a FocusProvider");
  }
  return context;
};
