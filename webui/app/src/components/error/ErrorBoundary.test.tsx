import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ErrorBoundary } from './ErrorBoundary.js';

describe('ErrorBoundary', () => {
  it('renders children when no error occurs', () => {
    render(() => (
      <ErrorBoundary>
        <div>Test content</div>
      </ErrorBoundary>
    ));

    expect(screen.getByText('Test content')).toBeInTheDocument();
  });

  it('renders custom fallback when provided', () => {
    const ErrorComponent = () => {
      throw new Error('Test error');
    };

    const customFallback = (error: unknown, _reset: () => void) => (
      <div>Custom error: {error instanceof Error ? error.message : 'Unknown error'}</div>
    );

    render(() => (
      <ErrorBoundary fallback={customFallback}>
        <ErrorComponent />
      </ErrorBoundary>
    ));

    expect(screen.getByText('Custom error: Test error')).toBeInTheDocument();
  });
});
