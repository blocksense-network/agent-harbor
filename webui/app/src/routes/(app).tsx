/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */
import { RouteSectionProps } from '@solidjs/router';

import { Navbar } from '../components/layout/Navbar';
import { Footer } from '../components/layout/Footer';

export default function AppLayout(props: RouteSectionProps) {
  return (
    <div class="flex h-screen flex-col bg-white">
      <Navbar />
      <main id="main" class="flex-1 overflow-hidden">
        {props.children}
      </main>
      <Footer />
    </div>
  );
}
