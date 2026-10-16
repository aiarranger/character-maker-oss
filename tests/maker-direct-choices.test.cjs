const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const Module = require('node:module');
const root = path.resolve(__dirname, '..');
const resolve = Module._resolveFilename;
Module._resolveFilename = function (name, parent, ...rest) {
  return resolve.call(this, name.startsWith('@/') ? path.join(root, name.slice(2)) : name, parent, ...rest);
};
for (const suffix of ['.ts', '.tsx']) require.extensions[suffix] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
require.extensions['.css'] = module => { module.exports = new Proxy({}, { get: (_, key) => String(key) }); };
const { test } = require('node:test');
const assert = require('node:assert/strict');
const React = require('react');
const { renderToStaticMarkup } = require('react-dom/server');
const { WeaponPicker } = require('../components/character-maker/WeaponPicker.tsx');
const { MAKER_WEAPON_CHOICES } = require('../lib/character-maker/weapon-catalog.ts');
const { requestWeaponForMotion, motionAfterWeaponChoice } = require('../lib/character-maker/weapon-motion-intent.ts');

test('all weapon and motion entries are direct choices, and unarmed requests retain ready/swing intent without assigning a weapon', () => {
  const ordinary = renderToStaticMarkup(React.createElement(WeaponPicker, { value: 'none', onChange() {} }));
  assert.equal((ordinary.match(/<button /g) ?? []).length, 13);
  assert.equal((ordinary.match(/<img /g) ?? []).length, 12);
  assert.equal((ordinary.match(/aria-pressed="true"/g) ?? []).length, 1);
  assert.doesNotMatch(ordinary, /<select|<option|role="listbox"/);
  for (const item of MAKER_WEAPON_CHOICES) {
    assert.ok(ordinary.includes(`aria-label="${item.name}"`));
    if (item.id !== 'none') assert.ok(fs.existsSync(path.join(root, 'public/review-swing-2026-10-02/weapons', `${item.id}.png`)));
  }
  for (const motion of ['sword_idle', 'sword_attack']) {
    const pending = requestWeaponForMotion(motion, 'none');
    assert.equal(pending, motion);
    assert.equal(motionAfterWeaponChoice('baguette_sword', pending), motion);
    assert.equal(motionAfterWeaponChoice('none', pending), 'idle');
    assert.equal(requestWeaponForMotion(motion, 'cake_sword'), null);
    const prompt = renderToStaticMarkup(React.createElement(WeaponPicker, { value: 'none', requestedMotion: pending, onChange() {}, onCancelRequest() {} }));
    assert.ok(prompt.includes('使う武器をクリックすると'));
    assert.ok(prompt.includes('キャンセル'));
    assert.ok(prompt.includes('role="status"'));
  }
  for (const motion of ['idle', 'walk', 'run']) assert.equal(requestWeaponForMotion(motion, 'none'), null);
  assert.equal(motionAfterWeaponChoice('cake_sword', null), 'sword_idle');
  for (const name of ['MakerPresetEditor.tsx', 'OriginalCharacterMaker.tsx']) {
    const source = fs.readFileSync(path.join(root, 'components/character-maker', name), 'utf8');
    assert.doesNotMatch(source, /<select|<option|disabled=\{weaponId === 'none'/);
    assert.ok(source.includes('requestWeaponForMotion('));
    assert.ok(source.includes('motionAfterWeaponChoice(value, pendingWeaponMotion)'));
    assert.ok(source.includes('weaponPicker.current?.focusChoices()'));
    assert.ok(source.includes('onCancelRequest={() => setPendingWeaponMotion(null)}'));
  }
  const preset = fs.readFileSync(path.join(root, 'components/character-maker/MakerPresetEditor.tsx'), 'utf8');
  assert.ok(preset.includes("setMotion(chosenWeaponMotion.current ?? (weaponId === 'none' ? 'idle' : 'sword_idle'))"));
});
