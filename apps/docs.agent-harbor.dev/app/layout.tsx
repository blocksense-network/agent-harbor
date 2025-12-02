/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { Layout, Navbar } from 'nextra-theme-docs';
import { Head } from 'nextra/components';
import { getPageMap } from 'nextra/page-map';
import { ReactNode } from 'react';
import { Metadata } from 'next';
import { Manrope } from 'next/font/google';

import './globals.css';
import { Logo } from '../components/Logo';
import { docsRepositoryBase, projectLink } from '../constants';

const manrope = Manrope({
  subsets: ['latin'],
});

export const metadata: Metadata = {};

const navbar = <Navbar logo={<Logo />} projectLink={projectLink} />;

type RootLayoutProps = {
  children: ReactNode;
};

const LastUpdated = () => <span />;

export default async function RootLayout({ children }: RootLayoutProps) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning className={manrope.className}>
      <Head></Head>
      <body>
        <Layout
          navbar={navbar}
          pageMap={await getPageMap()}
          docsRepositoryBase={docsRepositoryBase}
          lastUpdated={<LastUpdated />}
        >
          {children}
        </Layout>
      </body>
    </html>
  );
}
