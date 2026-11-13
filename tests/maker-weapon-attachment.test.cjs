const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { MAKER_WEAPON_SOCKETS } = require('../lib/character-maker/weapon-socket.ts');
const { MAKER_WEAPON_CHOICES } = require('../lib/character-maker/weapon-catalog.ts');
const registry = require('../public/review-swing-2026-10-02/weapons.json');

test('choices and every GLB/thumbnail are the exact registered main-list catalog', () => {
  assert.deepEqual(MAKER_WEAPON_CHOICES.slice(1), registry.weapons.map(({ id, name }) => ({ id, name })));
  for (const item of registry.weapons) for (const [key, hashKey] of [['model', 'model'], ['thumb', 'thumb']]) {
    const bytes = fs.readFileSync(path.join(__dirname, '../public/review-swing-2026-10-02', item[key]));
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), item.sha256[hashKey]);
  }
});

test('measured fits are attachment-only and expose nominal versus calibrated length', () => {
  // 武器の木（createMakerSword）と形の手直しは Rust の rust/game/src/weapon.rs へ移した。ここは値の表だけを見る
  const { makerWeaponFit } = require('../lib/character-maker/weapon-fit.ts');
  const expected = [['generic-human-a', .7], ['generic-human-b', .7], ['fantasy-feminine', .65], ['fantasy-masculine', .65], ['comic-feminine', .75], ['comic-masculine', .75]];
  for (const [rig, scale] of expected) {
    const fit = makerWeaponFit('tamago_pan', rig);
    assert.equal(fit.sizeScale, scale); assert.deepEqual(fit.gripOffset, [-.015, -.025, -.02]);
    assert.ok(fit.visualScale > 1.5);
    assert.equal(fit.effectiveScale, fit.visualScale);
  }
  for (const rig of ['generic-robot', 'generic-cat']) assert.equal(makerWeaponFit('tamago_pan', rig).sizeScale, 1);
  assert.equal(makerWeaponFit('baguette_sword', 'generic-human-a').sizeScale, 1);
});

test('protected grip keeps existing fist dimensions while blade/head retains uniform enlargement', () => {
  const { makerWeaponFit, makerWeaponGripCompensation } = require('../lib/character-maker/weapon-fit.ts');
  const fit = makerWeaponFit('cake_sword', 'generic-human-a');
  for (const y of [-.045, -.02, 0, .02, .045]) {
    assert.ok(Math.abs(fit.visualScale * fit.sizeScale * makerWeaponGripCompensation(y, fit) - fit.sizeScale) < 1e-12);
  }
  for (const y of [-.5, -.13, .13, .5]) {
    assert.ok(Math.abs(fit.visualScale * fit.sizeScale * makerWeaponGripCompensation(y, fit) - fit.visualScale) < 1e-12);
  }
  let previous = 0;
  for (let i = 0; i <= 100; i++) {
    const y = .045 + .085 * i / 100, compensation = makerWeaponGripCompensation(y, fit);
    assert.ok(Number.isFinite(compensation) && compensation >= previous); previous = compensation;
  }
  assert.equal(fit.geometryTransform, 'protected-grip-uniform-enlargement-v1');
  assert.ok(Math.abs(.7 * fit.visualScale / fit.referenceBodyHeight - .72 * fit.reference.protectedGripAllowance) < 1e-12);
});
