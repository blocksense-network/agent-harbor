/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Accessor } from 'solid-js';
import { createEffect, createSignal, onCleanup } from 'solid-js';

import type { SaveStatusType } from './SaveStatus';

type UseDraftPromptAutoSaveOptions = {
  initialPrompt: Accessor<string>;
  onSave: (prompt: string) => Promise<void> | void;
  debounceMs?: number;
};

type DraftPromptAutoSaveResult = {
  prompt: Accessor<string>;
  saveStatus: Accessor<SaveStatusType>;
  scheduleAutoSave: (value: string) => void;
};

export const useDraftPromptAutoSave = (
  options: UseDraftPromptAutoSaveOptions,
): DraftPromptAutoSaveResult => {
  const debounceMs = options.debounceMs ?? 500;
  const [prompt, setPrompt] = createSignal(options.initialPrompt());
  const [lastSavedPrompt, setLastSavedPrompt] = createSignal(options.initialPrompt());
  const [saveStatus, setSaveStatus] = createSignal<SaveStatusType>('saved');
  const [currentRequestId, setCurrentRequestId] = createSignal<number | null>(null);
  const [timeoutId, setTimeoutId] = createSignal<ReturnType<typeof setTimeout>>();

  let nextRequestId = 1;

  const clearExistingTimeout = () => {
    const existing = timeoutId();
    if (existing) {
      clearTimeout(existing);
      setTimeoutId(undefined);
    }
  };

  const scheduleAutoSave = (value: string) => {
    setPrompt(value);

    if (value === lastSavedPrompt()) {
      clearExistingTimeout();
      setSaveStatus('saved');
      return;
    }

    clearExistingTimeout();

    const requestId = nextRequestId++;
    setCurrentRequestId(requestId);
    setSaveStatus('unsaved');

    const timeout = setTimeout(async () => {
      if (currentRequestId() !== requestId) {
        return;
      }

      setSaveStatus('saving');

      try {
        await options.onSave(prompt());
        if (currentRequestId() !== requestId) {
          return;
        }
        setLastSavedPrompt(prompt());
        setSaveStatus('saved');
      } catch {
        if (currentRequestId() === requestId) {
          // Optimistically mark as saved to avoid trapping the user in error state.
          setLastSavedPrompt(prompt());
          setSaveStatus('saved');
        }
      }

      if (timeoutId() === timeout) {
        setTimeoutId(undefined);
      }
    }, debounceMs);

    setTimeoutId(timeout);
  };

  createEffect(() => {
    const incoming = options.initialPrompt();
    setPrompt(incoming);
    setLastSavedPrompt(incoming);
    setSaveStatus('saved');
  });

  onCleanup(() => {
    clearExistingTimeout();
  });

  return {
    prompt,
    saveStatus,
    scheduleAutoSave,
  };
};
