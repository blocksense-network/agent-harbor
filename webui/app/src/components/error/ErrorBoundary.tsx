import { Component, ErrorBoundary as SolidErrorBoundary, JSX } from "solid-js";

interface ErrorBoundaryProps {
  children: JSX.Element;
  fallback?: (error: any, reset: () => void) => JSX.Element;
}

const DefaultErrorFallback: Component<{ error: any; reset: () => void }> = (props) => {
  return (
    <div class="min-h-screen flex items-center justify-center bg-gray-50">
      <div class="max-w-md w-full bg-white rounded-lg shadow-lg p-6 text-center">
        <div class="w-16 h-16 bg-red-100 rounded-full flex items-center justify-center mx-auto mb-4">
          <span class="text-3xl text-red-600">⚠️</span>
        </div>
        <h1 class="text-xl font-semibold text-gray-900 mb-2">
          Something went wrong
        </h1>
        <p class="text-gray-600 mb-6">
          An unexpected error occurred. Please try refreshing the page or contact support if the problem persists.
        </p>
        <div class="space-y-3">
          <button
            onClick={props.reset}
            class="w-full bg-blue-600 text-white px-4 py-2 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 transition-colors"
          >
            Try Again
          </button>
          <button
            onClick={() => {
              if (typeof window !== "undefined") {
                window.location.reload();
              }
            }}
            class="w-full bg-gray-200 text-gray-800 px-4 py-2 rounded-md hover:bg-gray-300 focus:outline-none focus:ring-2 focus:ring-gray-500 transition-colors"
          >
            Refresh Page
          </button>
        </div>
        {process.env.NODE_ENV === "development" && (
          <details class="mt-4 text-left">
            <summary class="cursor-pointer text-sm text-gray-500 hover:text-gray-700 select-none">
              Error Details (Development)
            </summary>
            <div class="mt-2 space-y-2">
              <div>
                <label class="text-xs font-medium text-gray-700">Error Message:</label>
                <pre class="text-xs bg-gray-100 p-2 rounded overflow-auto max-h-16 select-text cursor-text">
                  {props.error?.message || "Unknown error"}
                </pre>
              </div>
              {props.error?.stack && (
                <div>
                  <label class="text-xs font-medium text-gray-700">Stack Trace:</label>
                  <pre class="text-xs bg-gray-100 p-2 rounded overflow-auto max-h-32 select-text cursor-text whitespace-pre-wrap">
                    {props.error.stack}
                  </pre>
                </div>
              )}
              {props.error?.cause && (
                <div>
                  <label class="text-xs font-medium text-gray-700">Cause:</label>
                  <pre class="text-xs bg-gray-100 p-2 rounded overflow-auto max-h-16 select-text cursor-text">
                    {JSON.stringify(props.error.cause, null, 2)}
                  </pre>
                </div>
              )}
            </div>
          </details>
        )}
      </div>
    </div>
  );
};

export const ErrorBoundary: Component<ErrorBoundaryProps> = (props) => {
  const fallback = (error: any, reset: () => void) => {
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
