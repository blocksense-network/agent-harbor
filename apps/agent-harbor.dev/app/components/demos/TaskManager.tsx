/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { IconSettings, IconFolder, IconBook, IconCpu, IconCloud } from '../icons/Icon';

interface Task {
  id: string;
  title: string;
  repo: string;
  branch: string;
  agent: string;
  timeAgo: string;
  status: 'active' | 'inactive';
  steps: string[];
}

const tasks: Task[] = [
  {
    id: '1',
    title: 'Implement payment processing',
    repo: 'ecommerce-platform',
    branch: 'feature/payments',
    agent: 'claude-3-5 (x1)',
    timeAgo: '5 min ago',
    status: 'active',
    steps: [
      'Evaluating performance implications',
      'Implementing proper error handling and logging',
    ],
  },
  {
    id: '2',
    title: 'Optimize database queries',
    repo: 'analytics-platform',
    branch: 'main',
    agent: 'gpt-4 (x1)',
    timeAgo: '10 min ago',
    status: 'inactive',
    steps: ['Considering edge cases...'],
  },
];

export default function TaskManager() {
  return (
    <div className="bg-gray-975 rounded-xl shadow-2xl overflow-hidden border border-gray-800 aspect-square sm:aspect-video lg:aspect-square transform -rotate-1 hover:rotate-0 transition-transform duration-500 font-mono text-[10px] hover:border-brand/30 flex flex-col leading-relaxed">
      <div className="flex justify-between items-center p-4 border-b border-gray-800/50">
        <div className="flex items-center gap-2 opacity-70">
          <span className="font-bold tracking-tighter text-gray-300 text-xs">agent-harbor</span>
        </div>
        <div className="text-gray-600 flex items-center gap-1 hover:text-gray-400 transition-colors cursor-pointer">
          <IconSettings />
          Settings
        </div>
      </div>

      <div className="p-4 flex flex-col h-full overflow-hidden">
        <NewTaskForm />
        <TaskList tasks={tasks} />
      </div>

      <div className="mt-auto pt-2 pb-2 px-4 border-t border-gray-800 text-gray-600 flex gap-4 text-[9px] bg-[#0d1117]">
        <span>
          <span className="text-gray-400 font-bold">Enter</span> Launch draft
        </span>
        <span>
          <span className="text-gray-400 font-bold">Shift+Enter</span> Insert newline
        </span>
        <span>
          <span className="text-gray-400 font-bold">Ctrl+?</span> Shortcut help
        </span>
      </div>
    </div>
  );
}

function NewTaskForm() {
  return (
    <div className="relative border border-brand/50 rounded-lg p-3 mb-6 bg-[#0d1117] group">
      <div className="absolute -top-2 left-2 bg-[#0d1117] px-1 text-brand text-[10px] font-bold tracking-wide">
        New Task
      </div>
      <div className="h-12 text-gray-500 pt-1">
        Describe what you want the agent to do...
        <span className="animate-pulse text-brand">|</span>
      </div>
      <div className="flex gap-2 mt-2 text-[10px] items-center">
        <Badge icon="folder" label="agent-harbor" />
        <Badge icon="book" label="main" brand />
        <Badge icon="cpu" label="claude-3-5-sonnet (x1)" purple />
        <div className="ml-auto text-gray-600 text-[9px] flex items-center gap-1 border border-gray-800 rounded px-1.5 py-0.5">
          ⏎ Go
        </div>
      </div>
    </div>
  );
}

function Badge({
  icon,
  label,
  brand,
  purple,
}: {
  icon: 'folder' | 'book' | 'cpu';
  label: string;
  brand?: boolean;
  purple?: boolean;
}) {
  const iconColor = brand ? 'text-brand' : purple ? 'text-purple-400' : 'text-gray-500';

  return (
    <span className="bg-gray-800/80 px-2 py-1 rounded text-gray-300 flex items-center gap-1 border border-gray-700">
      {icon === 'folder' && <IconFolder className={`w-3 h-3 ${iconColor}`} />}
      {icon === 'book' && <IconBook className={`w-3 h-3 ${iconColor}`} />}
      {icon === 'cpu' && <IconCpu className={`w-3 h-3 ${iconColor}`} />}
      {label}
    </span>
  );
}

function TaskList({ tasks }: { tasks: Task[] }) {
  return (
    <>
      <div className="flex gap-4 text-gray-600 mb-3 text-[10px] items-center border-b border-gray-800/50 pb-2">
        <span className="text-gray-400 font-bold">Existing tasks</span>
        <span className="hover:text-gray-300 cursor-pointer">Repo [All]</span>
        <span className="hover:text-gray-300 cursor-pointer">Status [All]</span>
        <span className="hover:text-gray-300 cursor-pointer">Creator [All]</span>
      </div>

      <div className="space-y-3 flex-1 overflow-hidden relative">
        {tasks.map(task => (
          <TaskCard key={task.id} task={task} />
        ))}
      </div>
    </>
  );
}

function TaskCard({ task }: { task: Task }) {
  return (
    <div
      className={`relative border border-gray-800 rounded-lg p-4 bg-[#0d1117] hover:border-gray-700 transition-colors ${
        task.status === 'inactive' ? 'opacity-60' : ''
      }`}
    >
      <div className="absolute -top-2 left-2 bg-[#0d1117] px-1 text-gray-300 text-[10px] font-bold border-x border-gray-950">
        {task.title}
      </div>
      <div className="flex justify-between items-start mb-3">
        <div className="text-gray-500 flex flex-wrap gap-2 items-center text-[9px]">
          <span className="w-1.5 h-1.5 rounded-full bg-orange-500 animate-pulse"></span>
          <span className="text-gray-400">{task.repo}</span>
          <span className="text-gray-600">•</span>
          <span className="text-gray-400">{task.branch}</span>
          <span className="text-gray-600">•</span>
          <span className="text-gray-400">{task.agent}</span>
          <span className="text-gray-600">•</span>
          <span>{task.timeAgo}</span>
        </div>
        <button className="bg-red-900/20 text-red-400 px-2 py-0.5 rounded text-[9px] hover:bg-red-900/30 transition-colors border border-red-900/30">
          Stop
        </button>
      </div>
      <div className="space-y-1.5 pl-1">
        {task.steps.map((step, index) => (
          <div key={index} className="flex gap-2 items-center text-gray-300">
            <IconCloud className="w-3 h-3 text-gray-500" />
            {step}
          </div>
        ))}
      </div>
    </div>
  );
}
