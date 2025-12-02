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
  os: 'macos' | 'linux';
}

const tasks: Task[] = [
  {
    id: '1',
    title: 'Validate Patch #1249 (MacOS)',
    repo: 'agent-harbor',
    branch: 'fix/race-condition',
    agent: 'test-runner-mac',
    timeAgo: 'Running',
    status: 'active',
    steps: ['Running integration tests...', 'Verifying FS snapshots'],
    os: 'macos',
  },
  {
    id: '2',
    title: 'Validate Patch #1249 (Linux)',
    repo: 'agent-harbor',
    branch: 'fix/race-condition',
    agent: 'test-runner-linux',
    timeAgo: 'Running',
    status: 'active',
    steps: ['Building Docker container...', 'Installing dependencies'],
    os: 'linux',
  },
];

export default function MultiOSValidation() {
  return (
    <div className="bg-gray-975 rounded-xl shadow-2xl overflow-hidden border border-gray-800 aspect-auto sm:aspect-square lg:aspect-square transform rotate-1 hover:rotate-0 transition-transform duration-500 font-mono text-[12px] hover:border-brand/30 flex flex-col leading-relaxed">
      <div className="flex justify-between items-center p-4 border-b border-gray-800/50">
        <div className="flex items-center gap-2 opacity-70">
          <span className="font-bold tracking-tighter text-gray-300 text-xs">validation-suite</span>
        </div>
        <div className="text-gray-600 flex items-center gap-1 hover:text-gray-400 transition-colors cursor-pointer">
          <IconSettings />
          Config
        </div>
      </div>

      <div className="p-4 flex flex-col h-full overflow-hidden">
        <ValidationHeader />
        <TaskList tasks={tasks} />
      </div>

      <div className="mt-auto pt-2 pb-2 px-4 border-t border-gray-800 text-gray-600 flex gap-4 text-[9px] bg-[#0d1117]">
        <span>
          <span className="text-gray-400 font-bold">Running</span> 2 jobs
        </span>
        <span>
          <span className="text-gray-400 font-bold">Queued</span> 0 jobs
        </span>
      </div>
    </div>
  );
}

function ValidationHeader() {
  return (
    <div className="relative border border-brand/50 rounded-lg p-3 mb-6 bg-[#0d1117] group">
      <div className="absolute -top-2 left-2 bg-[#0d1117] px-1 text-brand text-[10px] font-bold tracking-wide">
        Active Suite
      </div>
      <div className="h-12 text-gray-500 pt-1 flex items-center gap-2">
        <span className="text-gray-300">Validating pull request</span>
        <span className="text-brand">#1249</span>
        <span className="text-gray-500">on all targets</span>
      </div>
      <div className="flex gap-2 mt-2 text-[10px] items-center">
        <Badge icon="folder" label="agent-harbor" />
        <Badge icon="book" label="fix/race-condition" brand />
        <div className="ml-auto text-green-400 text-[9px] flex items-center gap-1 border border-green-900/30 bg-green-900/10 rounded px-1.5 py-0.5">
          ● Live
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
    <div className="space-y-3 flex-1 overflow-hidden relative pt-2">
      {tasks.map(task => (
        <TaskCard key={task.id} task={task} />
      ))}
    </div>
  );
}

function TaskCard({ task }: { task: Task }) {
  return (
    <div
      className={`relative border border-gray-800 rounded-lg p-4 bg-[#0d1117] hover:border-gray-700 transition-colors ${
        task.status === 'inactive' ? 'opacity-60' : ''
      }`}
    >
      <div className="absolute -top-2 left-2 bg-[#0d1117] px-1 text-gray-300 text-[10px] font-bold border-x border-gray-950 flex gap-2 items-center">
        {task.title}
      </div>
      <div className="flex justify-between items-start mb-3">
        <div className="text-gray-500 flex flex-wrap gap-2 items-center text-[9px]">
          <span className="w-1.5 h-1.5 rounded-full bg-orange-500 animate-pulse"></span>
          <span className="text-gray-400">{task.os === 'macos' ? 'macOS-14' : 'ubuntu-22.04'}</span>
          <span className="text-gray-600">•</span>
          <span className="text-gray-400">{task.agent}</span>
        </div>
        <div className="text-[9px] text-brand animate-pulse">{task.timeAgo}</div>
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
