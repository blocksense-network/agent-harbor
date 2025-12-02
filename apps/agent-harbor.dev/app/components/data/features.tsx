/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { ReactElement } from 'react';
import SandboxDemo from '../demos/SandboxDemo';
import AgentDashboard from '../demos/AgentDashboard';
import TaskManager from '../demos/TaskManager';
import { IconSandbox, IconFilter, IconAntiHalting } from '../icons/Icon';

export interface Feature {
  icon: ReactElement;
  title: string;
  features: string[];
  demo: ReactElement;
  reverse?: boolean;
}

export const features: Feature[] = [
  {
    icon: <IconSandbox />,
    title: 'A sandbox built for YOLO mode',
    features: [
      'Near-instant agent startup and perfect environment replication in a copy-on-write file system',
      'Roll back to any local snapshot and fork agents to continue prompting with your agent/model of choice',
    ],
    demo: <SandboxDemo />,
  },
  {
    icon: <IconAntiHalting />,
    title: 'Anti-halting features for long-horizon tasks',
    features: [
      'Automated diagnosis of stuck processes to overcome deadlocks, infinite loops and connection failures in test suites',
      'Supervisor agent conducts targeted web research to overcome challenges and re-prompt for higher quality responses',
    ],
    demo: <TaskManager />,
    reverse: true,
  },
  {
    icon: <IconFilter />,
    title: 'Designed for professional development',
    features: [
      'Let agents run test suites and validate patches on all operating systems in parallel with advanced leader/follower orchestration',
      'Every task, command, and transcript can be recorded for later review, debugging, and compliance',
    ],
    demo: <AgentDashboard />,
  },
];
