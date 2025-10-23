/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import { RouteSectionProps, useLocation, A } from '@solidjs/router';
import { For, onCleanup, onMount } from 'solid-js';

import agentHarborLogo from '../assets/agent-harbor-logo.svg';
import { Footer } from '../components/layout/Footer';
import { useDrafts } from '../contexts/DraftContext.js';
import { useFocus } from '../contexts/FocusContext.js';
import { useBreadcrumbs } from '../contexts/BreadcrumbContext.js';

export default function AppLayout(props: RouteSectionProps) {
  const location = useLocation();
  const { focusState } = useFocus();
  const { breadcrumbs } = useBreadcrumbs();
  const draftOps = useDrafts();

  const isActive = (path: string) => location.pathname === path;

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
      <a
        href="#main"
        class={`
          sr-only z-50 rounded-md bg-blue-600 px-4 py-2 text-white
          focus:not-sr-only focus:absolute focus:top-2 focus:left-2
        `}
      >
        Skip to main content
      </a>

      <header class="border-b border-slate-200 bg-white px-6 py-3 shadow-sm">
        <div class="flex items-center">
          <div class="flex items-center space-x-3">
            <img
              src={agentHarborLogo}
              alt="Agent Harbor Logo"
              class="h-8 w-8"
              width="32"
              height="32"
            />
            <h1 class="text-xl font-bold text-gray-900">Agent Harbor</h1>
          </div>

          <div class="flex flex-1 justify-center">
            {breadcrumbs().length > 0 && (
              <nav
                class="flex items-center space-x-1 text-xs text-gray-500"
                aria-label="Breadcrumb"
              >
                <For each={breadcrumbs()}>
                  {(crumb, index) => (
                    <>
                      {index() > 0 && <span class="mx-1">/</span>}
                      {crumb.href ? (
                        <A
                          href={crumb.href}
                          class={`
                            transition-colors
                            hover:text-blue-600 hover:underline
                          `}
                        >
                          {crumb.label}
                        </A>
                      ) : crumb.onClick ? (
                        <button
                          onClick={crumb.onClick}
                          class={`
                            transition-colors
                            hover:text-blue-600 hover:underline
                          `}
                        >
                          {crumb.label}
                        </button>
                      ) : (
                        <span class="font-medium text-gray-700">{crumb.label}</span>
                      )}
                    </>
                  )}
                </For>
              </nav>
            )}
          </div>

          <nav class="flex space-x-1" aria-label="Primary">
            <A
              href="/settings"
              class={`
                rounded-lg px-4 py-2 text-sm font-medium transition-colors
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              classList={{
                'bg-slate-100 text-slate-900': isActive('/settings'),
                'text-slate-600 hover:text-slate-900 hover:bg-slate-100': !isActive('/settings'),
              }}
              aria-current={location.pathname === '/settings' ? 'page' : undefined}
            >
              <span aria-hidden="true">⚙️</span> Settings
            </A>
          </nav>
        </div>
      </header>

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
