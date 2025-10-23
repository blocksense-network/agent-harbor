/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { A, useLocation } from '@solidjs/router';

import agentHarborLogo from '../../assets/agent-harbor-logo.svg';
import { Breadcrumbs } from './Breadcrumbs';

export const Navbar = () => {
  const location = useLocation();

  const isActive = (path: string) => location.pathname === path;

  return (
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
        <Breadcrumbs />
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
  );
};
