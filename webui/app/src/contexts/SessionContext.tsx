import { createContext, useContext, createSignal, Component, JSX } from 'solid-js';

interface SessionContextValue {
  selectedSessionId: () => string | undefined;
  setSelectedSessionId: (id: string | undefined) => void;
}

const SessionContext = createContext<SessionContextValue>();

interface SessionProviderProps {
  children: JSX.Element;
}

export const SessionProvider: Component<SessionProviderProps> = props => {
  const [selectedSessionId, setSelectedSessionId] = createSignal<string | undefined>();

  const value: SessionContextValue = {
    selectedSessionId,
    setSelectedSessionId,
  };

  return <SessionContext.Provider value={value}>{props.children}</SessionContext.Provider>;
};

export const useSession = () => {
  const context = useContext(SessionContext);
  if (!context) {
    throw new Error('useSession must be used within a SessionProvider');
  }
  return context;
};
