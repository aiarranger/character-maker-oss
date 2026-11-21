import assert from 'node:assert/strict';
import test from 'node:test';
import { assertGameBuilt, storeGameCharacter, STUDIO_GAMES } from './game-handoff.ts';

for (const [id, g] of Object.entries(STUDIO_GAMES)) {
  // 防ぐ故障: public/<id> が無い（pnpm <id>:build をしていない）のに、黙って 404 のページへ飛ぶ
  test(`${id}: the game page must exist before leaving the Studio`, async () => {
    const asked = [];
    await assertGameBuilt(id, async (input, init) => { asked.push([input, init.method]); return { ok: true }; });
    assert.deepEqual(asked, [[g.page, 'HEAD']]);
    await assert.rejects(assertGameBuilt(id, async () => ({ ok: false })), { message: g.notBuilt });
    await assert.rejects(assertGameBuilt(id, async () => { throw new TypeError('offline'); }), { message: g.notBuilt });
  });

  // 防ぐ故障: Studio が置くキーと、ゲーム（games/<id>/rust/web/index.html）が読むキーがずれて、送ったキャラが出ない
  test(`${id}: the character zip is stored under the one key the game reads`, async () => {
    const puts = [];
    const store = { open: async name => ({ put: async (key, response) => { puts.push([name, key, response.headers.get('content-type'), new Uint8Array(await response.arrayBuffer())]); } }) };
    await storeGameCharacter(id, new Blob([new Uint8Array([80, 75, 3, 4])]), store);
    assert.deepEqual(puts, [[g.cacheName, g.characterKey, 'application/zip', new Uint8Array([80, 75, 3, 4])]]);
    const page = await (await import('node:fs/promises')).readFile(new URL(`../../games/${id}/rust/web/index.html`, import.meta.url), 'utf8');
    assert.ok(page.includes(`caches.open('${g.cacheName}')`), 'index.html opens the same cache');
    assert.ok(page.includes("new URL('studio-character.zip', location.href).pathname") && g.characterKey === `/${id}/studio-character.zip` && g.page === `/${id}/index.html`, 'index.html reads the same key');
    assert.equal(g.playUrl, `/${id}/index.html?from=studio`);
    await assert.rejects(storeGameCharacter(id, new Blob([]), undefined), { message: g.noStore });
  });
}

// 魚群繚乱の控えの名前は前と同じ（送ったキャラが、作り替えの前後で読めなくならない）
test('the gyogun cache keeps its name', () => {
  assert.equal(STUDIO_GAMES.gyogun.cacheName, 'character-maker-gyogun-v1');
  assert.equal(STUDIO_GAMES.gyogun.notBuilt, '魚群繚乱がまだ作られていません。`pnpm gyogun:build` を実行してから、もう一度押してください。');
});
