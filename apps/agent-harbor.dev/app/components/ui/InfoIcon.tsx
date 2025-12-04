/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

interface InfoIconProps {
  text: string;
}

export function InfoIcon({ text }: InfoIconProps) {
  return (
    <span className="inline-block ml-2 [&:has(label:hover)_>_div]:opacity-100 [&:has(label:hover)_>_div]:visible [&:has(label:hover)_>_div]:pointer-events-auto">
      <input
        type="checkbox"
        id={text}
        className="peer sr-only"
        aria-label="Toggle more information"
      />
      <label
        htmlFor={text}
        className="inline-block transition-colors w-fit focus:outline-none p-1 rounded cursor-pointer bg-gray-800/50 text-gray-500 peer-checked:bg-gray-800/70 peer-checked:text-gray-400 hover:bg-gray-800/70 hover:text-gray-400"
        aria-label="More information"
      >
        <span className="text-sm">ⓘ</span>
      </label>
      <div className="absolute left-0 top-full mt-2 z-50 max-w-xs sm:max-w-sm p-3 bg-gray-800 border border-gray-700 rounded-lg shadow-lg text-xs text-gray-300 whitespace-normal opacity-0 invisible pointer-events-none transition-opacity peer-checked:opacity-100 peer-checked:visible peer-checked:pointer-events-auto">
        {text}
      </div>
    </span>
  );
}
