// 公開用の書き出し（tools/gyogun-site.mjs）の確かめ方。docs/gyogun-spec.md の「公開先」
const test = require('node:test');
const assert = require('node:assert/strict');
const { mkdtempSync, mkdirSync, writeFileSync, truncateSync, rmSync, readFileSync } = require('node:fs');
const { join, dirname } = require('node:path');
const { tmpdir } = require('node:os');

const load = () => import('../tools/gyogun-site.mjs');
function fixture(files) {
  const dir = mkdtempSync(join(tmpdir(), 'gyogun-site-'));
  for (const [path, bytes] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), '');
    if (bytes) truncateSync(join(dir, path), bytes);
  }
  return dir;
}

// 防ぐ故障: 公開版で / や Studio・魚群繚乱・模型が 404 になる。Workers の 1 ファイル 25 MiB を超えて公開できない。古い出来上がり（public/muso）や静的書き出しで動かない旧画面を配る
test('the site check finds missing pages, files over 25 MiB and things that must not be served', async () => {
  const { inspectSite, REQUIRED, MAX_FILE, REDIRECTS } = await load();
  const good = Object.fromEntries(REQUIRED.map(path => [path, 10]));
  const ok = fixture({ ...good, 'models/big.glb': MAX_FILE });
  try { assert.deepEqual(inspectSite(ok).errors, []); } finally { rmSync(ok, { recursive: true }); }
  const bad = fixture({ ...good, 'models/too-big.glb': MAX_FILE + 1, 'muso/index.html': 10, 'character-maker/classic.html': 10 });
  const missing = { ...good }; delete missing['gyogun/index.html'];
  const gone = fixture(missing);
  try {
    const errors = inspectSite(bad).errors.join('\n');
    assert.match(errors, /25 MiB を超える: models\/too-big\.glb/);
    assert.match(errors, /入れない物がある: muso/);
    assert.match(errors, /入れない物がある: character-maker\/classic\.html/);
    assert.match(inspectSite(gone).errors.join('\n'), /無い: gyogun\/index\.html/);
  } finally { rmSync(bad, { recursive: true }); rmSync(gone, { recursive: true }); }
  // / は魚群繚乱へ
  assert.equal(REDIRECTS, '/ /gyogun/ 302\n');
  // 公開の設定は、書き出し先と同じ場所を配る（サーバーの処理は無い）
  const wrangler = readFileSync(join(__dirname, '..', 'wrangler.jsonc'), 'utf8');
  assert.match(wrangler, /"directory": "\.\/out"/);
  assert.match(wrangler, /"pattern": "gyogun\.yukikau\.jp", "custom_domain": true/);
  assert.doesNotMatch(wrangler, /"main"/);
});
