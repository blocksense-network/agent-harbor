import {
  createContext,
  createSignal,
  useContext,
  Component,
  JSX,
} from "solid-js";
import {
  Toast,
  ToastType,
  ToastAction,
  ToastContainer,
} from "../components/common/Toast.js";

interface ToastContextValue {
  toasts: () => Toast[];
  addToast: (type: ToastType, message: string, duration?: number) => void;
  addToastWithActions: (
    type: ToastType,
    message: string,
    actions?: ToastAction[],
    duration?: number,
  ) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue>();

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return context;
};

interface ToastProviderProps {
  children: JSX.Element;
}

export const ToastProvider: Component<ToastProviderProps> = (props) => {
  const [toasts, setToasts] = createSignal<Toast[]>([]);

  const addToast = (type: ToastType, message: string, duration?: number) => {
    const id = Math.random().toString(36).substr(2, 9);
    const toast: Toast = { id, type, message, duration };
    setToasts((prev) => [...prev, toast]);
  };

  const addToastWithActions = (
    type: ToastType,
    message: string,
    actions?: ToastAction[],
    duration?: number,
  ) => {
    const id = Math.random().toString(36).substr(2, 9);
    const toast: Toast = { id, type, message, duration, actions };
    setToasts((prev) => [...prev, toast]);
  };

  const removeToast = (id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  };

  const value: ToastContextValue = {
    toasts,
    addToast,
    addToastWithActions,
    removeToast,
  };

  return (
    <ToastContext.Provider value={value}>
      {props.children}
      <ToastContainer toasts={toasts()} onRemove={removeToast} />
    </ToastContext.Provider>
  );
};
