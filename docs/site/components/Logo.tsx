/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import Image from 'next/image';

import lightLogo from '../public/logos/light_logo.png';
import darkLogo from '../public/logos/dark_logo.png';

export const Logo = () => {
  return (
    <>
      <Image
        src={lightLogo}
        alt="Agent Harbor Logo"
        loading="eager"
        width={220}
        className=" hidden dark:block"
      />
      <Image
        src={darkLogo}
        alt="Agent Harbor Logo"
        loading="eager"
        width={220}
        className="block dark:hidden"
      />
    </>
  );
};
