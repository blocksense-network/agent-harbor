/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export default function SandboxDemo() {
  return (
    <div className="bg-gray-975 rounded-xl shadow-2xl overflow-hidden border border-gray-800 aspect-square sm:aspect-video lg:aspect-square transform rotate-1 hover:rotate-0 transition-transform duration-500 font-mono text-[15px] hover:border-brand/30 flex flex-col leading-relaxed">
      <div className="bg-gray-950 px-4 py-3 flex items-center gap-2 border-b border-gray-800">
        <div className="w-2.5 h-2.5 rounded-full bg-red-500/80"></div>
        <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/80"></div>
        <div className="w-2.5 h-2.5 rounded-full bg-green-500/80"></div>
        <span className="ml-2 text-xs text-gray-500 font-mono opacity-60">sandbox-cow — zsh</span>
      </div>

      <div className="p-4 flex flex-col h-full overflow-hidden bg-[#0d1117] text-gray-300">
        <div className="font-mono text-[15px] space-y-1">
          <p className="flex gap-2">
            <span className="text-purple-400">➜</span>
            <span className="text-gray-300">ah</span>
          </p>
          <p className="text-gray-500">Starting copy-on-write environment...</p>
          <p className="text-gray-500">Replicating local configs... Done (12ms)</p>
          <p className="text-gray-500">Snapshot 0d8F2A created.</p>
          <p className="text-brand animate-pulse">Ready for autonomous execution.</p>
          <p className="flex gap-2 pt-1">
            <span className="text-purple-400">➜</span>
            <span className="text-gray-300">ah task --agent claude --prompt "Create a new file 'hello.py' that prints 'Hello, Agent Harbor!'"</span>
          </p>
          <div className="text-gray-400 pl-4 border-l border-gray-800 ml-1">
            
          </div>
        </div>
      </div>

      <div className="mt-auto pt-2 pb-2 px-4 border-t border-gray-800 text-gray-600 flex gap-4 text-[9px] bg-[#0d1117]">
        {/* <span>
          <span className="text-gray-400 font-bold">FS</span> CoW
        </span>
        <span>
          <span className="text-gray-400 font-bold">Snapshot</span> Active
        </span>
        <span className="ml-auto flex items-center gap-1">
          <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse"></span>
          Online
        </span> */}
      </div>
    </div>
  );
}
