/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import type { Metadata } from 'next';
import { Manrope, JetBrains_Mono } from 'next/font/google';
import './globals.css';
import { siteName, siteUrl, opengraphImageUrl, xUrl } from './lib/constants';

const manrope = Manrope({
  subsets: ['latin'],
  variable: '--font-manrope',
  display: 'swap',
});

const jetbrainsMono = JetBrains_Mono({
  subsets: ['latin'],
  variable: '--font-jetbrains-mono',
  display: 'swap',
});

export const metadata: Metadata = {
  title: siteName,
  description:
    'A powerful YOLO-mode harness for long-horizon vibe engineering, enabling local orchestration of Claude, Gemini, and Codex with advanced sandboxing and instant rollbacks.',
  openGraph: {
    url: siteUrl,
    images: [
      {
        url: opengraphImageUrl,
        width: 1200,
        height: 630,
        secureUrl: opengraphImageUrl,
        alt: siteName,
      },
    ],
    type: 'website',
    siteName,
  },
  twitter: {
    site: xUrl,
    images: [opengraphImageUrl],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="dark">
      <body className={`${manrope.variable} ${jetbrainsMono.variable}`}>{children}</body>
    </html>
  );
}
