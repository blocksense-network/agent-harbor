/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { Layout, Navbar } from 'nextra-theme-docs';
import { Head } from 'nextra/components';
import { getPageMap } from 'nextra/page-map';
import { Manrope } from 'next/font/google';

import './globals.css';
import { Logo } from '../components/Logo';

const manrope = Manrope({
  subsets: ['latin'],
});

export const metadata = {};

const navbar = <Navbar logo={<Logo />} />;

export default async function RootLayout({ children }) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning className={manrope.className}>
      <Head></Head>
      <body>
        <Layout
          navbar={navbar}
          pageMap={await getPageMap()}
          docsRepositoryBase="https://github.com/blocksense-network/agent-harbor"
        >
          {children}
        </Layout>
      </body>
    </html>
  );
}
