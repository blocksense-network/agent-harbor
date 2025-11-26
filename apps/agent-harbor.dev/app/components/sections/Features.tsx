/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import FeatureItem from '../ui/FeatureItem';
import { features } from '../data/features';

export default function Features() {
  return (
    <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 space-y-24 pb-32 relative z-10">
      {features.map((feature, index) => (
        <FeatureItem key={index} {...feature} />
      ))}
    </div>
  );
}
