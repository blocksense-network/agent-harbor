/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import Image from 'next/image';
import { IconTwitter } from '../icons/Icon';
import { xUrl } from '../../lib/constants';

export default function Footer() {
  return (
    <footer className="bg-gray-950 pt-16 pb-12 border-t border-gray-800">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row justify-between items-center gap-8">
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <Image
              src="/logo.png"
              alt="Agent Harbor Logo"
              width={32}
              height={32}
              className="object-contain"
            />
          </div>
          <div className="mt-2">
            <span className="font-mono font-bold block text-white">Agent Harbor</span>
          </div>
        </div>

        <div className="flex items-center gap-6">
          {/* <a
            href="#"
            className="text-gray-500 hover:text-brand transition-colors"
            aria-label="Discord"
          >
            <IconDiscord />
          </a> */}
          <a
            href={xUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="text-gray-500 hover:text-brand transition-colors"
            aria-label="X (Twitter)"
          >
            <IconTwitter />
          </a>
          {/* <a
            href="#"
            className="text-gray-500 hover:text-brand transition-colors"
            aria-label="GitHub"
          >
            <IconGitHub />
          </a> */}
        </div>
      </div>
    </footer>
  );
}
