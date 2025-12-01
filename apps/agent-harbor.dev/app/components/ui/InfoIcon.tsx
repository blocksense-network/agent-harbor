/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

'use client';

import { useState } from 'react';

interface InfoIconProps {
  text: string;
}

export function InfoIcon({ text }: InfoIconProps) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <>
      <button
        type="button"
        onClick={e => {
          if (e.target instanceof HTMLSpanElement) {
            e.target.blur();
            setIsOpen(!isOpen);
          }
        }}
        onMouseEnter={() => setIsOpen(true)}
        onMouseLeave={() => setIsOpen(false)}
        onBlur={() => setIsOpen(false)}
        className={`inline-block ml-2 transition-colors w-fit focus:outline-none p-1 rounded ${
          isOpen ? 'bg-gray-800/70 text-gray-400' : 'bg-gray-800/50 text-gray-500'
        }`}
        aria-label="More information"
        aria-expanded={isOpen}
      >
        <span className="text-sm">ⓘ</span>
      </button>
      <div
        className={`absolute left-0 top-full mt-2 z-50 max-w-xs sm:max-w-sm p-3 bg-gray-800 border border-gray-700 rounded-lg shadow-lg text-xs text-gray-300 whitespace-normal ${
          isOpen
            ? 'opacity-100 visible pointer-events-auto'
            : 'opacity-0 invisible pointer-events-none'
        }`}
      >
        {text}
      </div>
    </>
  );
}
