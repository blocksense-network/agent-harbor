/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');

const iconsDir = path.resolve(projectRoot, 'src/assets/icons');
const outputPath = path.resolve(projectRoot, 'src/components/common/iconManifest.ts');

async function readIconNames(directory: string): Promise<string[]> {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const iconNames = entries
    .filter(entry => entry.isFile() && entry.name.toLowerCase().endsWith('.svg'))
    .map(entry => entry.name.replace(/\.svg$/i, ''))
    .sort((a, b) => a.localeCompare(b));

  return iconNames;
}

function buildManifestContent(iconNames: string[]): string {
  const lines = iconNames.map(name => `  '${name}',`).join('\n');
  return `export const icon_variants = [\n${lines}\n] as const;\n\nexport type IconVariant = (typeof icon_variants)[number];\n`;
}

async function writeManifest(filePath: string, content: string): Promise<void> {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, content, 'utf8');
}

async function main(): Promise<void> {
  try {
    const iconNames = await readIconNames(iconsDir);
    const content = buildManifestContent(iconNames);
    await writeManifest(outputPath, content);
    console.log(`Generated icon manifest with ${iconNames.length} variant(s).`);
  } catch (error) {
    console.error('Failed to generate icon manifest:', error);
    process.exitCode = 1;
  }
}

await main();
