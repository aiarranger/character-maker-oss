'use client';
import { useEffect, useRef } from 'react';

type ToolRegistry = {
  registerTool(tool: {
    name: string; title: string; description: string; inputSchema: object;
    annotations: { readOnlyHint: boolean; untrustedContentHint: boolean };
    execute: (input: unknown) => unknown;
  }, options: { signal: AbortSignal }): void | Promise<void>;
};

export function useCharacterReadTool(value: unknown) {
  const latest = useRef(value);
  latest.current = value;
  useEffect(() => {
    const context = (window.document as Document & { modelContext?: ToolRegistry }).modelContext;
    if (!context?.registerTool) return;
    const lifecycle = new AbortController();
    try {
      void Promise.resolve(context.registerTool({
        name: 'read_character_edit',
        title: '現在のキャラクター設定を読む',
        description: 'Return the current visible character editing document without changing or saving it.',
        inputSchema: { type: 'object', properties: {}, additionalProperties: false },
        annotations: { readOnlyHint: true, untrustedContentHint: true },
        execute(input) {
          if (input === null || typeof input !== 'object' || Array.isArray(input) || Object.keys(input).length) {
            throw new Error('Expected an empty object.');
          }
          return JSON.parse(JSON.stringify(latest.current));
        },
      }, { signal: lifecycle.signal })).catch(() => {});
    } catch { /* This optional browser capability must not interrupt editing. */ }
    return () => lifecycle.abort();
  }, []);
}
