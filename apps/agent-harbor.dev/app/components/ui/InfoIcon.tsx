/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

interface InfoIconProps {
  text: string;
}

export function InfoIcon({ text }: InfoIconProps) {
  return (
    <>
      <button
        type="button"
        className="peer inline-block ml-2 text-gray-500 hover:text-gray-400 transition-colors focus:outline-none p-1 rounded bg-gray-800/50 hover:bg-gray-800/70"
        aria-label="More information"
      >
        <span className="text-sm">ⓘ</span>
      </button>
      <div className="absolute left-0 top-full mt-2 z-50 max-w-xs sm:max-w-sm p-3 bg-gray-800 border border-gray-700 rounded-lg shadow-lg text-xs text-gray-300 whitespace-normal opacity-0 invisible pointer-events-none peer-hover:opacity-100 peer-hover:visible peer-hover:pointer-events-auto hover:opacity-100 hover:visible hover:pointer-events-auto transition-opacity duration-200">
        {text}
      </div>
    </>
  );
}
