/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export const icon_variants = ['arrow-down', 'close', 'minus', 'models', 'plus'] as const;

export type IconVariant = (typeof icon_variants)[number];
