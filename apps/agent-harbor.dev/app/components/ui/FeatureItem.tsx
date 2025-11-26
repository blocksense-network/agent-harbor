/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { ReactNode } from 'react';
import { IconCheck } from '../icons/Icon';

interface FeatureItemProps {
  icon: ReactNode;
  title: string;
  features: string[];
  demo: ReactNode;
  reverse?: boolean;
}

export default function FeatureItem({
  icon,
  title,
  features,
  demo,
  reverse = false,
}: FeatureItemProps) {
  return (
    <div
      className={`flex flex-col ${reverse ? 'lg:flex-row-reverse' : 'lg:flex-row'} items-center gap-12 lg:gap-20`}
    >
      <div className="flex-1 space-y-6">
        <div className="h-12 w-12 bg-brand-light rounded-xl flex items-center justify-center text-brand mb-4 border border-brand/20">
          {icon}
        </div>
        <h3 className="text-3xl font-bold text-white">{title}</h3>
        <ul className="space-y-4">
          {features.map((item, index) => (
            <li key={index} className="flex gap-3">
              <IconCheck className="w-6 h-6 text-brand shrink-0" />
              <span className="text-gray-400 text-lg">{item}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="flex-1 w-full">{demo}</div>
    </div>
  );
}
