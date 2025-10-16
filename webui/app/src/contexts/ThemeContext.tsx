import {
  createContext,
  createSignal,
  useContext,
  Component,
  onMount,
  createEffect,
  JSX,
} from 'solid-js';

type Theme = 'light' | 'dark';

interface ThemeContextValue {
  theme: () => Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue>();

export const useTheme = () => {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
};

export const ThemeProvider: Component<{ children: JSX.Element }> = (props) => {
  const [theme, setTheme] = createSignal<Theme>('light');

  // Check if we're on the client side
  const isClient = typeof window !== 'undefined';

  // Load theme from localStorage on mount (client-side only)
  onMount(() => {
    if (!isClient) return;

    const savedTheme = localStorage.getItem('theme') as Theme;
    if (savedTheme && (savedTheme === 'light' || savedTheme === 'dark')) {
      setTheme(savedTheme);
    } else {
      // Check system preference
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      setTheme(prefersDark ? 'dark' : 'light');
    }
  });

  const toggleTheme = () => {
    const newTheme = theme() === 'light' ? 'dark' : 'light';
    setTheme(newTheme);

    if (isClient) {
      localStorage.setItem('theme', newTheme);
      // Apply theme to document
      applyTheme(newTheme);
    }
  };

  const applyTheme = (theme: Theme) => {
    if (!isClient) return;

    const root = document.documentElement;
    if (theme === 'dark') {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  };

  // Apply theme when it changes (client-side only)
  createEffect(() => {
    if (isClient) {
      applyTheme(theme());
    }
  });

  const value: ThemeContextValue = {
    theme,
    setTheme,
    toggleTheme,
  };

  return <ThemeContext.Provider value={value}>{props.children}</ThemeContext.Provider>;
};
