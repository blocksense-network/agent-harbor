/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, Show } from 'solid-js';
import { A } from '@solidjs/router';

import { useBreadcrumbs } from '../../contexts/BreadcrumbContext.jsx';

export const Breadcrumbs = () => {
  const { breadcrumbs } = useBreadcrumbs();

  return (
    <div class="flex flex-1 justify-center">
      <Show when={breadcrumbs().length > 0}>
        <nav class="flex items-center space-x-1 text-xs text-gray-500" aria-label="Breadcrumb">
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
      </Show>
    </div>
  );
};
