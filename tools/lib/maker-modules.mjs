// 確認用の偽オリジンのページ（/maker-mature/ など）へ、lib/character-maker の TypeScript をそのまま配る。
// assembly.ts が import する先（JSON や public/ の武器一覧を含む）を、増えても許可リストを直さずに配れるようにする。
import fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { NODE_MODULES, ROOT } from './paths.mjs';

const LIB = path.join(ROOT, 'lib/character-maker');

function moduleFile(rel) {
  if (rel.startsWith('three/')) return path.resolve(NODE_MODULES, rel);
  if (/^[a-z0-9-]+$/.test(rel)) return [`${rel}.ts`, `${rel}.mjs`].map(name => path.join(LIB, name)).find(existsSync) ?? null;
  if (/^[a-z0-9-]+\.(json|mjs)$/.test(rel)) return path.join(LIB, rel);
  return null;
}

async function fulfillFile(route, filename) {
  if (!filename || !existsSync(filename)) return route.fulfill({ status: 404 });
  const source = await fs.readFile(filename, 'utf8');
  const body = filename.endsWith('.ts') ? ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
    : filename.endsWith('.json') ? `export default ${source};` : source;
  return route.fulfill({ contentType: 'text/javascript', body });
}

/** /<prefix>/<rel> の rel（three/… か lib/character-maker のモジュール名）を配る */
export function fulfillMakerModule(route, rel) {
  return fulfillFile(route, moduleFile(rel));
}

/** lib から ../../public/…json を import したときの要求（/public/…）を、JSON モジュールとして配る */
export function routePublicJson(page) {
  return page.route(/\/public\/[^?]+\.json(\?.*)?$/, route => {
    const rel = decodeURIComponent(new URL(route.request().url()).pathname.slice('/public/'.length));
    const filename = path.resolve(ROOT, 'public', rel);
    return fulfillFile(route, filename.startsWith(path.join(ROOT, 'public') + path.sep) ? filename : null);
  });
}
