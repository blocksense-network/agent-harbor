import { Component, ErrorBoundary as SolidErrorBoundary, JSX } from "solid-js";

interface ErrorBoundaryProps {
  children: JSX.Element;
  fallback?: (error: unknown, reset: () => void) => JSX.Element;
}

const DefaultErrorFallback: Component<{ error: unknown; reset: () => void }> = (
  props,
) => {
  return (
    <div class="flex min-h-screen items-center justify-center bg-gray-50">
      <div class="w-full max-w-md rounded-lg bg-white p-6 text-center shadow-lg">
        <div
          class="
          mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full
          bg-red-100
        "
        >
          <span class="text-3xl text-red-600">⚠️</span>
        </div>
        <h1 class="mb-2 text-xl font-semibold text-gray-900">
          Something went wrong
        </h1>
        <p class="mb-6 text-gray-600">
          An unexpected error occurred. Please try refreshing the page or
          contact support if the problem persists.
        </p>
        <div class="space-y-3">
          <button
            onClick={props.reset}
            class={`
              w-full rounded-md bg-blue-600 px-4 py-2 text-white
              transition-colors
              hover:bg-blue-700
              focus:ring-2 focus:ring-blue-500 focus:outline-none
            `}
          >
            Try Again
          </button>
          <button
            onClick={() => {
              if (typeof window !== "undefined") {
                window.location.reload();
              }
            }}
            class={`
              w-full rounded-md bg-gray-200 px-4 py-2 text-gray-800
              transition-colors
              hover:bg-gray-300
              focus:ring-2 focus:ring-gray-500 focus:outline-none
            `}
          >
            Refresh Page
          </button>
        </div>
        {process.env["NODE_ENV"] === "development" && (
          <details class="mt-4 text-left">
            <summary
              class={`
                cursor-pointer text-sm text-gray-500 select-none
                hover:text-gray-700
              `}
            >
              Error Details (Development)
            </summary>
            <div class="mt-2 space-y-2">
              <div>
                <label class="text-xs font-medium text-gray-700">
                  Error Message:
                </label>
                <pre
                  class={`
                    max-h-16 cursor-text overflow-auto rounded bg-gray-100 p-2
                    text-xs select-text
                  `}
                >
                  {props.error instanceof Error
                    ? props.error.message
                    : "Unknown error"}
                </pre>
              </div>
              {props.error instanceof Error && props.error.stack && (
                <div>
                  <label class="text-xs font-medium text-gray-700">
                    Stack Trace:
                  </label>
                  <pre
                    class={`
                      max-h-32 cursor-text overflow-auto rounded bg-gray-100 p-2
                      text-xs whitespace-pre-wrap select-text
                    `}
                  >
                    {props.error.stack}
                  </pre>
                </div>
              )}
              {props.error instanceof Error &&
              (props.error as Error & { cause?: unknown }).cause ? (
                <div>
                  <label class="text-xs font-medium text-gray-700">
                    Cause:
                  </label>
                  <pre class="max-h-16 cursor-text overflow-auto rounded bg-gray-100 p-2 text-xs select-text">
                    {String((props.error as Error & { cause?: unknown }).cause)}
                  </pre>
                </div>
              ) : null}
            </div>
          </details>
        )}
      </div>
    </div>
  );
};

export const ErrorBoundary: Component<ErrorBoundaryProps> = (props) => {
  const fallback = (error: unknown, reset: () => void) => {
    if (props.fallback) {
      return props.fallback(error, reset);
    }
    return <DefaultErrorFallback error={error} reset={reset} />;
  };

  return (
    <SolidErrorBoundary fallback={fallback}>
      {props.children}
    </SolidErrorBoundary>
  );
};
