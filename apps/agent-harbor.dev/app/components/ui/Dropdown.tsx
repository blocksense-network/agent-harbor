/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

'use client';

import { useState, useRef, useEffect } from 'react';

interface DropdownProps {
  value: string;
  onChange: (value: string) => void;
  options: string[];
  placeholder: string;
  required?: boolean;
}

export function Dropdown({ value, onChange, options, placeholder, required }: DropdownProps) {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setIsOpen(false);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      setIsOpen(!isOpen);
    } else if (e.key === 'ArrowDown' && !isOpen) {
      e.preventDefault();
      setIsOpen(true);
    }
  };

  const selectedOption = value || placeholder;

  return (
    <div ref={dropdownRef} className="relative">
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        onKeyDown={handleKeyDown}
        className={`w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-left focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors flex items-center justify-between font-mono ${
          !value ? 'text-gray-600' : 'text-white'
        }`}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-required={required}
      >
        <span className="text-left">{selectedOption}</span>
        <svg
          className={`w-5 h-5 transition-transform shrink-0 ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {isOpen && (
        <div className="absolute z-[9999] w-full mt-1 bg-gray-950 border border-gray-700 rounded-lg shadow-lg overflow-hidden">
          <div className="overflow-auto custom-scrollbar">
            {options.map(option => (
              <button
                key={option}
                type="button"
                onClick={() => {
                  onChange(option);
                  setIsOpen(false);
                }}
                className={`w-full px-4 py-3 text-left text-white transition-all duration-150 font-mono ${
                  value === option
                    ? 'bg-gray-800'
                    : 'bg-gray-950 hover:bg-gray-800 hover:text-gray-100'
                }`}
                role="option"
                aria-selected={value === option}
              >
                {option}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
