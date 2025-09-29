import { Component, createSignal, onMount, onCleanup, For } from "solid-js";

export type ToastType = "success" | "error" | "info" | "warning";

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}

interface ToastItemProps {
  toast: Toast;
  onRemove: (id: string) => void;
}

const ToastItem: Component<ToastItemProps> = (props) => {
  const [isVisible, setIsVisible] = createSignal(true);

  onMount(() => {
    const duration = props.toast.duration ?? 5000;
    const timer = setTimeout(() => {
      setIsVisible(false);
      setTimeout(() => props.onRemove(props.toast.id), 300); // Allow animation to complete
    }, duration);

    onCleanup(() => clearTimeout(timer));
  });

  const getToastStyles = (type: ToastType) => {
    const baseStyles = "flex items-center p-4 mb-4 text-sm rounded-lg transition-all duration-300";

    switch (type) {
      case "success":
        return `${baseStyles} bg-green-100 text-green-800 border border-green-200`;
      case "error":
        return `${baseStyles} bg-red-100 text-red-800 border border-red-200`;
      case "warning":
        return `${baseStyles} bg-yellow-100 text-yellow-800 border border-yellow-200`;
      case "info":
      default:
        return `${baseStyles} bg-blue-100 text-blue-800 border border-blue-200`;
    }
  };

  const getIcon = (type: ToastType) => {
    switch (type) {
      case "success":
        return "✓";
      case "error":
        return "✕";
      case "warning":
        return "⚠";
      case "info":
      default:
        return "ℹ";
    }
  };

  return (
    <div
      class={`toast-item ${getToastStyles(props.toast.type)} ${
        isVisible() ? "opacity-100 translate-x-0" : "opacity-0 translate-x-full"
      }`}
      role="alert"
      aria-live="assertive"
    >
      <div class="flex-shrink-0 mr-3">
        <span class="text-lg" aria-hidden="true">{getIcon(props.toast.type)}</span>
      </div>
      <div class="flex-1 font-medium">
        {props.toast.message}
      </div>
      <button
        onClick={() => props.onRemove(props.toast.id)}
        class="ml-3 flex-shrink-0 text-current hover:opacity-75 transition-opacity focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-blue-500 rounded"
        aria-label="Dismiss notification"
      >
        <span class="text-lg" aria-hidden="true">×</span>
      </button>
    </div>
  );
};

interface ToastContainerProps {
  toasts: Toast[];
  onRemove: (id: string) => void;
}

export const ToastContainer: Component<ToastContainerProps> = (props) => {
  return (
    <div
      class="fixed top-4 right-4 z-50 max-w-sm"
      role="region"
      aria-label="Notifications"
    >
      <For each={props.toasts}>
        {(toast) => <ToastItem toast={toast} onRemove={props.onRemove} />}
      </For>
    </div>
  );
};
