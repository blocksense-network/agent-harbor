/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import { RouteSectionProps } from '@solidjs/router';
import { onCleanup, onMount } from 'solid-js';

import { Navbar } from '../components/layout/Navbar';
import { Footer } from '../components/layout/Footer';
import { useDrafts } from '../contexts/DraftContext';
import { useFocus } from '../contexts/FocusContext';

export default function AppLayout(props: RouteSectionProps) {
  const { focusState } = useFocus();
  const draftOps = useDrafts();

  const handleNewDraft = async () => {
    if (!import.meta.env.PROD) {
      console.log('[AppLayout] New Task button clicked');
    }

    const created = await draftOps.createDraft({
      prompt: '',
      repo: { mode: 'git', url: '', branch: 'main' },
      agents: [],
      runtime: { type: 'devcontainer' },
      delivery: { mode: 'pr' },
    });

    if (!import.meta.env.PROD) {
      console.log('[AppLayout] Draft creation result:', created);
    }
  };

  onMount(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) {
        return;
      }

      if (event.key.toLowerCase() !== 'n') {
        return;
      }

      const target = event.target;
      if (target && target instanceof window.HTMLElement) {
        const tagName = target.tagName?.toLowerCase();
        const isEditable =
          tagName === 'input' || tagName === 'textarea' || target.isContentEditable;
        if (isEditable) {
          return;
        }
      }

      event.preventDefault();
      void handleNewDraft();
    };

    window.addEventListener('keydown', handleShortcut);
    onCleanup(() => window.removeEventListener('keydown', handleShortcut));
  });

  return (
    <div class="flex h-screen flex-col bg-white">
      <Navbar />
      <main id="main" class="flex-1 overflow-hidden">
        {props.children}
      </main>
      {(() => {
        const state = focusState();
        return (
          <Footer
            onNewDraft={handleNewDraft}
            focusState={state}
            agentCount={state.focusedDraftAgentCount}
          />
        );
      })()}
    </div>
  );
}
