/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

interface CSSStyleDeclaration {
  backgroundColor: string;
}

declare function getComputedStyle(elt: Element, pseudoElt?: string | null): CSSStyleDeclaration;
