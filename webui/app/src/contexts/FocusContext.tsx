import { createContext, useContext, Component, JSX, createSignal } from 'solid-js';

interface FocusState {
  focusedElement: 'draft-textarea' | 'session-card' | 'none';
  focusedDraftId?: string;
  focusedSessionId?: string;
  focusedDraftAgentCount?: number;
}

interface FocusContextValue {
  focusState: () => FocusState;
  setDraftFocus: (draftId: string, agentCount?: number) => void;
  setSessionFocus: (sessionId: string) => void;
  updateDraftAgentCount: (draftId: string, agentCount: number) => void;
  clearFocus: () => void;
  isDraftFocused: (draftId: string) => boolean;
  isSessionFocused: (sessionId: string) => boolean;
}

const FocusContext = createContext<FocusContextValue>();

export const FocusProvider: Component<{ children: JSX.Element }> = props => {
  const [focusState, setFocusState] = createSignal<FocusState>({
    focusedElement: 'none',
  });

  const setDraftFocus = (draftId: string, agentCount?: number) => {
    setFocusState({
      focusedElement: 'draft-textarea',
      focusedDraftId: draftId,
      ...(agentCount !== undefined && {
        focusedDraftAgentCount: agentCount,
      }),
    });
  };

  const setSessionFocus = (sessionId: string) => {
    setFocusState({
      focusedElement: 'session-card',
      focusedSessionId: sessionId,
    });
  };

  const clearFocus = () => {
    setFocusState({
      focusedElement: 'none',
    });
  };

  const isDraftFocused = (draftId: string) => {
    const state = focusState();
    return state.focusedElement === 'draft-textarea' && state.focusedDraftId === draftId;
  };

  const isSessionFocused = (sessionId: string) => {
    const state = focusState();
    return state.focusedElement === 'session-card' && state.focusedSessionId === sessionId;
  };

  const updateDraftAgentCount = (draftId: string, agentCount: number) => {
    setFocusState(prev => {
      if (prev.focusedElement === 'draft-textarea' && prev.focusedDraftId === draftId) {
        return {
          ...prev,
          focusedDraftAgentCount: agentCount,
        };
      }
      return prev;
    });
  };

  const contextValue: FocusContextValue = {
    focusState,
    setDraftFocus,
    setSessionFocus,
    updateDraftAgentCount,
    clearFocus,
    isDraftFocused,
    isSessionFocused,
  };

  return <FocusContext.Provider value={contextValue}>{props.children}</FocusContext.Provider>;
};

export const useFocus = (): FocusContextValue => {
  const context = useContext(FocusContext);
  if (!context) {
    throw new Error('useFocus must be used within a FocusProvider');
  }
  return context;
};
