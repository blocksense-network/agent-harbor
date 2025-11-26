/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { IconLock, IconShield, IconTerminal, IconCode } from '../icons/Icon';

export default function Benefits() {
  const benefits = [
    {
      icon: <IconLock />,
      title: 'PRIVATE',
      description:
        "Agent Harbor does not collect any data from model usage and you bring your own subscription, so data never enters Agent Harbor's servers",
    },
    {
      icon: <IconShield />,
      title: 'SECURE',
      description:
        'Customize the sandbox at file system and network level to granularly control what agents have access to. Fully auditable workflow.',
    },
    {
      icon: <IconTerminal />,
      title: 'FLEXIBLE',
      description:
        "Agent Harbor runs where you work. Launch the multiplexer from your favorite terminal emulator - whether it's Ghostty, Emacs, LazyVim, or just plain vanilla Terminal.",
    },
    {
      icon: <IconCode />,
      title: 'OPEN SOURCE',
      description:
        'Fully open sourced and extensible. See our guidelines if you would like to learn how to contribute.',
    },
  ];

  return (
    <section className="bg-gray-900 border-t border-gray-800 py-24 relative z-10">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          {benefits.map((benefit, index) => (
            <div
              key={index}
              className="bg-gray-950 p-6 rounded-2xl border border-gray-800 hover:border-brand/40 hover:shadow-[0_0_20px_rgba(0,255,247,0.1)] transition-all group h-full"
            >
              <div className="w-10 h-10 rounded-lg bg-gray-900 border border-gray-800 flex items-center justify-center text-brand mb-4 group-hover:bg-brand/10 transition-colors">
                {benefit.icon}
              </div>
              <h4 className="text-lg font-bold text-white mb-3">{benefit.title}</h4>
              <p className="text-gray-400 text-m leading-relaxed">{benefit.description}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
