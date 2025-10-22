/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For } from 'solid-js';

const highlightSyntax = (code: string) => {
  if (!code || code.trim() === '') {
    return <span>{code || '\u00A0'}</span>;
  }

  const keywords = [
    'fn',
    'let',
    'mut',
    'if',
    'else',
    'for',
    'while',
    'loop',
    'match',
    'return',
    'struct',
    'enum',
    'impl',
    'trait',
    'use',
    'mod',
    'pub',
    'crate',
    'super',
    'Self',
    'self',
    'true',
    'false',
    'Some',
    'None',
    'Ok',
    'Err',
    'Result',
    'Box',
    'Vec',
    'HashMap',
    'String',
    'println',
    'eprintln',
    'format',
    'std',
    'collections',
    'io',
    'error',
    'process',
    'env',
    'args',
    'collect',
    'exit',
    'path',
    'Path',
    'exists',
    'fs',
    'read_to_string',
    'dyn',
    'Error',
    'into',
    'as',
    'ref',
  ];

  const types = ['i32', 'u32', 'i64', 'u64', 'usize', 'isize', 'f32', 'f64', 'bool', 'char'];

  const tokens = code
    .split(/(\s+|[{}();,.=<>!+\-*/&|?:[\]]|\w+|"[^"]*"|'[^']*'|\/\/.*)/g)
    .filter(token => token !== undefined && token !== '');

  return (
    <span style={{ 'white-space': 'pre', 'tab-size': '4' }}>
      <For each={tokens}>
        {(token: string) => {
          if (keywords.includes(token)) {
            return <span class="font-medium text-purple-600">{token}</span>;
          }
          if (types.includes(token)) {
            return <span class="font-medium text-orange-600">{token}</span>;
          }
          if (token.startsWith('"') && token.endsWith('"')) {
            return <span class="text-green-600">{token}</span>;
          }
          if (token.startsWith("'") && token.endsWith("'")) {
            return <span class="text-green-600">{token}</span>;
          }
          if (token.startsWith('//')) {
            return <span class="text-gray-500 italic">{token}</span>;
          }
          if (/^[{}();,.=<>!+\-*/&|?:[\]]+$/.test(token)) {
            return <span class="text-blue-500">{token}</span>;
          }
          return <span>{token}</span>;
        }}
      </For>
    </span>
  );
};

type DiffLineProps = {
  type: 'context' | 'addition' | 'deletion' | 'hunk';
  content: string;
  lineNumber?: number;
};

const parseDiff = (diffContent: string): DiffLineProps[] => {
  const lines = diffContent.split('\n');
  const result: DiffLineProps[] = [];
  let leftLineNumber = 0;
  let rightLineNumber = 0;

  for (const line of lines) {
    if (line.startsWith('@@')) {
      result.push({ type: 'hunk', content: line });
      continue;
    }

    if (line.startsWith('+')) {
      rightLineNumber++;
      result.push({ type: 'addition', content: line.substring(1), lineNumber: rightLineNumber });
      continue;
    }

    if (line.startsWith('-')) {
      leftLineNumber++;
      result.push({ type: 'deletion', content: line.substring(1), lineNumber: leftLineNumber });
      continue;
    }

    if (line.startsWith(' ')) {
      leftLineNumber++;
      rightLineNumber++;
      result.push({ type: 'context', content: line.substring(1), lineNumber: rightLineNumber });
      continue;
    }

    result.push({ type: 'context', content: line });
  }

  return result;
};

type DiffViewerProps = {
  content: string;
};

export const DiffViewer = (props: DiffViewerProps) => {
  const lines = () => parseDiff(props.content);
  const lineClasses = {
    hunk: 'bg-gray-100 text-gray-700 px-2 py-1 text-xs border-l-4 border-blue-400',
    addition: 'bg-green-50 text-green-800 border-l-4 border-green-400',
    deletion: 'bg-red-50 text-red-800 border-l-4 border-red-400',
    context: 'bg-gray-50 text-gray-700 border-l-4 border-gray-300',
  };

  return (
    <div class="font-mono text-sm leading-relaxed">
      <For each={lines()}>
        {line => (
          <div class="flex" classList={{ [lineClasses[line.type]]: true }}>
            <div class="w-12 pr-2 text-right text-gray-500 select-none">
              {line.lineNumber || ''}
            </div>
            <div class="flex-1 pl-2">
              {line.type === 'hunk' ? (
                <span class="font-bold">{line.content}</span>
              ) : (
                highlightSyntax(line.content || '\u00A0')
              )}
            </div>
          </div>
        )}
      </For>
    </div>
  );
};
