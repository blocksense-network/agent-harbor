/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export default function SandboxDemo() {
  return (
    <div className="bg-gray-900 rounded-xl shadow-2xl overflow-hidden border border-gray-800 aspect-square sm:aspect-video lg:aspect-square transform rotate-1 hover:rotate-0 transition-transform duration-500 hover:border-brand/30 hover:shadow-[0_0_30px_rgba(0,0,0,0.5)]">
      <div className="bg-gray-950 px-4 py-2 flex items-center gap-2 border-b border-gray-800">
        <div className="w-3 h-3 rounded-full bg-red-500"></div>
        <div className="w-3 h-3 rounded-full bg-yellow-500"></div>
        <div className="w-3 h-3 rounded-full bg-green-500"></div>
        <span className="ml-2 text-xs text-gray-500 font-mono">sandbox-cow — zsh</span>
      </div>
      <div className="p-6 font-mono text-sm text-brand leading-relaxed">
        <p className="mb-2">
          <span className="text-purple-400">➜</span> <span className="text-white">~</span> ah
        </p>
        <p className="text-gray-500 mb-4">Initializing CoW filesystem... Done (12ms)</p>
        <p className="text-gray-500 mb-4">Snapshot 0x8F2A created.</p>
        <div className="bg-gray-950/80 p-4 rounded border-l-2 border-brand">
          <p className="text-gray-300">Agent starting environment clone...</p>
          <p className="text-gray-300">Replicating local configs...</p>
          <p className="text-brand animate-pulse">Ready for autonomous execution.</p>
        </div>
      </div>
    </div>
  );
}
