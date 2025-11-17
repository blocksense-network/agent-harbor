/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import Image from 'next/image';

import logoIcon from '../public/icons/logo.svg';

export const Logo = () => {
  return (
    <div className="flex items-center gap-2">
      <Image src={logoIcon} alt="Agent Harbor Logo" />
      <span>agent harbor</span>
    </div>
  );
};
