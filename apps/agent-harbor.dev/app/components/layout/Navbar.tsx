/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import Image from 'next/image';

export default function Navbar() {
  return (
    <nav className="sticky top-0 z-50 border-b border-white/10 bg-gray-950/80 backdrop-blur-md">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex justify-between items-center h-20">
          <div className="flex items-center gap-3">
            <Image
              src="/logo.png"
              alt="Agent Harbor Logo"
              width={32}
              height={32}
              className="object-contain filter drop-shadow-[0_0_8px_rgba(0,255,247,0.4)]"
            />
            <span className="font-mono font-bold text-lg tracking-tight text-white">
              Agent Harbor
            </span>
          </div>
        </div>
      </div>
    </nav>
  );
}
