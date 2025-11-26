/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

interface AgentStatus {
  id: string;
  name: string;
  status: 'running' | 'completed' | 'best';
  progress?: number;
  task?: string;
}

const agents: AgentStatus[] = [
  {
    id: 'claude',
    name: 'Claude Opus 4.5',
    status: 'running',
    progress: 75,
  },
  {
    id: 'gpt',
    name: 'GPT 5.1 (high)',
    status: 'completed',
    progress: 100,
  },
  {
    id: 'gemini',
    name: 'Gemini 3',
    status: 'best',
    task: 'Refactoring api_routes.ts...',
  },
];

export default function AgentDashboard() {
  return (
    <div className="bg-gray-900 rounded-xl shadow-2xl overflow-hidden border border-gray-800 aspect-square sm:aspect-video lg:aspect-square transform -rotate-1 hover:rotate-0 transition-transform duration-500 relative group hover:border-brand/30">
      <div className="absolute inset-0 bg-brand/5 group-hover:bg-transparent transition-colors"></div>
      <div className="p-6 h-full flex flex-col">
        <div className="flex justify-between items-center mb-6">
          <h4 className="font-mono font-bold text-gray-500 uppercase text-xs tracking-wider">
            Agent Harbor
          </h4>
          <span className="px-2 py-1 bg-green-900/30 border border-green-500/30 text-green-400 text-xs font-bold rounded">
            In progress
          </span>
        </div>

        <div className="space-y-3 flex-1">
          {agents.map(agent => (
            <AgentCard key={agent.id} agent={agent} />
          ))}
        </div>
      </div>
    </div>
  );
}

function AgentCard({ agent }: { agent: AgentStatus }) {
  const isBest = agent.status === 'best';
  const isCompleted = agent.status === 'completed';
  const isRunning = agent.status === 'running';

  const colorClasses = isBest
    ? 'border-brand/40 bg-brand/5 shadow-[0_0_15px_rgba(0,255,247,0.1)] ring-1 ring-brand/50'
    : 'border-gray-700 bg-gray-800/50';

  const iconBg = isBest
    ? 'bg-brand/20 border-brand/30 text-brand'
    : isCompleted
      ? 'bg-green-900/30 border-green-500/20 text-green-400'
      : 'bg-purple-900/30 border-purple-500/20 text-purple-400';

  const iconLetter = agent.name.charAt(0);

  return (
    <div className={`p-4 rounded-lg border shadow-sm flex items-center gap-4 ${colorClasses}`}>
      <div
        className={`w-10 h-10 rounded flex items-center justify-center font-bold border ${iconBg}`}
      >
        {iconLetter}
      </div>
      <div className="flex-1">
        <div className="flex justify-between">
          <span className="font-bold text-sm text-gray-200">{agent.name}</span>
          {isRunning && <span className="text-xs text-gray-500">Running...</span>}
          {isCompleted && <span className="text-xs text-green-400">Completed</span>}
          {isBest && <span className="text-xs text-brand font-bold">Best Candidate</span>}
        </div>
        {agent.progress !== undefined && (
          <div className="w-full bg-gray-950 h-1.5 mt-2 rounded-full overflow-hidden">
            <div
              className={`h-full ${
                isCompleted ? 'bg-green-500' : 'bg-purple-500'
              } ${isRunning ? 'animate-pulse' : ''}`}
              style={{ width: `${agent.progress}%` }}
            />
          </div>
        )}
        {agent.task && <div className="text-xs text-gray-400 mt-1 font-mono">{agent.task}</div>}
      </div>
    </div>
  );
}
