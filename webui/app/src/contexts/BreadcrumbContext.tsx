import { createContext, createSignal, useContext, Component, JSX } from 'solid-js';

export interface BreadcrumbItem {
  label: string;
  href?: string;
  onClick?: () => void;
}

interface BreadcrumbContextValue {
  breadcrumbs: () => BreadcrumbItem[];
  setBreadcrumbs: (breadcrumbs: BreadcrumbItem[]) => void;
  clearBreadcrumbs: () => void;
}

const BreadcrumbContext = createContext<BreadcrumbContextValue>();

export const BreadcrumbProvider: Component<{ children: JSX.Element }> = props => {
  const [breadcrumbs, setBreadcrumbs] = createSignal<BreadcrumbItem[]>([]);

  const clearBreadcrumbs = () => setBreadcrumbs([]);

  const value: BreadcrumbContextValue = {
    breadcrumbs,
    setBreadcrumbs,
    clearBreadcrumbs,
  };

  return <BreadcrumbContext.Provider value={value}>{props.children}</BreadcrumbContext.Provider>;
};

export const useBreadcrumbs = () => {
  const context = useContext(BreadcrumbContext);
  if (!context) {
    throw new Error('useBreadcrumbs must be used within a BreadcrumbProvider');
  }
  return context;
};
