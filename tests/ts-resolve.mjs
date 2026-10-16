// node --import ./tests/ts-resolve.mjs で使う。lib の .ts は Next 用に拡張子なしで import しているので、
// Node で直接読むテスト・E2E のときだけ「.ts を補う」「JSON を読む」を足す。本体のコードは変えない。
import { registerHooks } from 'node:module';

const RELATIVE = /^\.{1,2}\//;
registerHooks({
  resolve(specifier, context, next) {
    try { return next(specifier, context); } catch (error) {
      if (error?.code !== 'ERR_MODULE_NOT_FOUND' || !RELATIVE.test(specifier)) throw error;
      for (const suffix of ['.ts', '/index.ts']) {
        try { return next(specifier + suffix, context); } catch { /* 次の候補 */ }
      }
      throw error;
    }
  },
  load(url, context, next) {
    // import x from './a.json'（with { type: 'json' } なし）を許す
    if (url.endsWith('.json') && !context.importAttributes?.type) return next(url, { ...context, importAttributes: { type: 'json' } });
    return next(url, context);
  },
});
