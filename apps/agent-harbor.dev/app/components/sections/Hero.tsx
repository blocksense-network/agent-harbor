/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

'use client';

import { scrollToElement } from '../../lib/utils';

export default function Hero() {
  return (
    <section className="relative z-10 pt-20 pb-24 lg:pt-32 lg:pb-32 overflow-hidden">
      <div className="max-w-4xl mx-auto px-4 text-center">
        <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-brand-light text-brand text-xs font-mono font-semibold mb-8 border border-brand/20">
          <span className="relative flex h-2 w-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-brand opacity-75"></span>
            <span className="relative inline-flex rounded-full h-2 w-2 bg-brand"></span>
          </span>
          V 1.0 loading...
        </div>

        <h1 className="text-5xl md:text-6xl font-extrabold tracking-tight text-white mb-6">
          YOLO Mode.
          <span className="text-transparent bg-clip-text bg-gradient-to-r from-brand to-cyan-200 neon-text">
            {' '}
            10x&apos;d.
          </span>
        </h1>

        <p className="text-2xl md:text-3xl font-mono text-gray-400 mb-8 font-medium">
          Stop typing. Start shipping.
        </p>

        <p className="text-lg text-gray-400 max-w-2xl mx-auto leading-relaxed mb-10">
          <span className="font-mono font-bold text-brand">Agent Harbor</span> is the vibe
          engineering harness built for long-horizon tasks. Orchestrate dozens of Claude, Claude,
          Gemini and Codex agents, with an advanced local sandbox and effortless rollbacks.
        </p>

        <div className="flex flex-col sm:flex-row justify-center gap-4">
          <button
            onClick={() => scrollToElement('early-access')}
            className="px-8 py-3 rounded-lg bg-brand text-black font-mono font-bold hover:bg-brand-hover transition-all shadow-[0_0_20px_rgba(0,255,247,0.4)] hover:shadow-[0_0_30px_rgba(0,255,247,0.6)] transform hover:-translate-y-0.5"
          >
            Be an early tester
          </button>
        </div>
      </div>
    </section>
  );
}
