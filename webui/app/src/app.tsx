/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { Router } from '@solidjs/router';
import { FileRoutes } from '@solidjs/start/router';
import { MetaProvider, Title, Meta } from '@solidjs/meta';
import { isServer, getRequestEvent } from 'solid-js/web';
import { Suspense } from 'solid-js';

import { SessionProvider } from './contexts/SessionContext';
import { DraftProvider } from './contexts/DraftContext';
import { FocusProvider } from './contexts/FocusContext';
import { ToastProvider } from './contexts/ToastContext';
import { BreadcrumbProvider } from './contexts/BreadcrumbContext';
import './app.css';

export default function App() {
  let initialUrl = '';
  if (isServer) {
    const event = getRequestEvent();
    if (event) {
      initialUrl = event.request.url;
    }
  }

  return (
    <MetaProvider>
      <Title>Agent Harbor</Title>
      <Meta name="description" content="Create and manage AI agent coding sessions" />
      <Meta name="viewport" content="width=device-width, initial-scale=1.0" />
      <ToastProvider>
        <SessionProvider>
          <DraftProvider>
            <FocusProvider>
              <BreadcrumbProvider>
                <Router url={initialUrl} root={props => <Suspense>{props.children}</Suspense>}>
                  <FileRoutes />
                </Router>
              </BreadcrumbProvider>
            </FocusProvider>
          </DraftProvider>
        </SessionProvider>
      </ToastProvider>
    </MetaProvider>
  );
}
