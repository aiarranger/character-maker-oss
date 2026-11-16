import assert from 'node:assert/strict';
import test from 'node:test';
import { assertGyogunBuilt, storeGyogunCharacter, GYOGUN_CACHE_NAME, GYOGUN_CHARACTER_KEY, GYOGUN_NOT_BUILT, GYOGUN_NO_STORE, GYOGUN_PAGE, GYOGUN_PLAY_URL } from './gyogun-handoff.ts';

// 防ぐ故障: public/gyogun が無い（pnpm gyogun:build をしていない）のに、黙って 404 のページへ飛ぶ
test('the game page must exist before leaving the Studio', async () => {
  const asked = [];
  await assertGyogunBuilt(async (input, init) => { asked.push([input, init.method]); return { ok: true }; });
  assert.deepEqual(asked, [[GYOGUN_PAGE, 'HEAD']]);
  await assert.rejects(assertGyogunBuilt(async () => ({ ok: false })), { message: GYOGUN_NOT_BUILT });
  await assert.rejects(assertGyogunBuilt(async () => { throw new TypeError('offline'); }), { message: GYOGUN_NOT_BUILT });
});

// 防ぐ故障: Studio が置くキーと、魚群繚乱（games/gyogun/rust/web/index.html）が読むキーがずれて、送ったキャラが出ない
test('the character zip is stored under the one key the game reads', async () => {
  const puts = [];
  const store = { open: async name => ({ put: async (key, response) => { puts.push([name, key, response.headers.get('content-type'), new Uint8Array(await response.arrayBuffer())]); } }) };
  await storeGyogunCharacter(new Blob([new Uint8Array([80, 75, 3, 4])]), store);
  assert.deepEqual(puts, [[GYOGUN_CACHE_NAME, GYOGUN_CHARACTER_KEY, 'application/zip', new Uint8Array([80, 75, 3, 4])]]);
  const page = await (await import('node:fs/promises')).readFile(new URL('../../games/gyogun/rust/web/index.html', import.meta.url), 'utf8');
  assert.ok(page.includes(`caches.open('${GYOGUN_CACHE_NAME}')`), 'index.html opens the same cache');
  assert.ok(page.includes("new URL('studio-character.zip', location.href).pathname") && GYOGUN_CHARACTER_KEY === '/gyogun/studio-character.zip' && GYOGUN_PAGE === '/gyogun/index.html', 'index.html reads the same key');
  assert.equal(GYOGUN_PLAY_URL, '/gyogun/index.html?from=studio');
  await assert.rejects(storeGyogunCharacter(new Blob([]), undefined), { message: GYOGUN_NO_STORE });
});
