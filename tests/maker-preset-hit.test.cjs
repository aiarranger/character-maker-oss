// のけぞり（hit）と ＞＜（face_hurt）のクリップの決まり。docs/main-character-spec.md
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { HIT_CLIP } = require('../lib/character-maker/hurt.ts');
const { makerAssetUrl } = require('../lib/character-maker/asset-url.ts');


test('the hit motion data are new files: one clip named hit for every preset body', () => {
  for (const style of ['fantasy', 'comic']) for (const base of ['feminine', 'masculine']) {
    const file = path.join(__dirname, `../public/maker-assets/${style}-${base}-hit-motions.json`);
    const clips = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.deepEqual(clips.map(clip => clip.name), [HIT_CLIP]);
    assert.ok(Math.abs(clips[0].duration - 0.6667) < 0.001);
    assert.ok(clips[0].tracks.length >= 50);
    // 作り直したら、ブラウザのキャッシュを避ける版（asset-url.ts）も変える。
    const sha = require('node:crypto').createHash('sha256').update(fs.readFileSync(file)).digest('hex').slice(0, 16);
    assert.ok(makerAssetUrl(`${style}-${base}-hit-motions.json`).endsWith(`?v=${sha}`), `${style}-${base}: cache revision matches the file`);
  }
});
