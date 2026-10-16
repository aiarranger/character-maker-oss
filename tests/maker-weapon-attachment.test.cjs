const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { Group, Mesh, BoxGeometry, MeshBasicMaterial, AnimationClip, Vector3 } = require('three');
const { createMakerSword, fitMakerWeaponScale, addWeaponTracks, disposeMakerWeapon, MAKER_WEAPON_SOCKETS } = require('../lib/character-maker/weapon.ts');
const { MAKER_WEAPON_CHOICES } = require('../lib/character-maker/weapon-catalog.ts');
const registry = require('../public/review-swing-2026-10-02/weapons.json');

test('choices and every GLB/thumbnail are the exact registered main-list catalog', () => {
  assert.deepEqual(MAKER_WEAPON_CHOICES.slice(1), registry.weapons.map(({ id, name }) => ({ id, name })));
  for (const item of registry.weapons) for (const [key, hashKey] of [['model', 'model'], ['thumb', 'thumb']]) {
    const bytes = fs.readFileSync(path.join(__dirname, '../public/review-swing-2026-10-02', item[key]));
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), item.sha256[hashKey]);
  }
});

test('registered content is privately cloned, retaining source assets and sockets while deriving visible-size geometry', () => {
  const source = new Group(); source.userData.registeredWeapon = { id: 'baguette_sword', length: .75 };
  const geometry = new BoxGeometry(.01, .75, .01), material = new MeshBasicMaterial({ color: 0xabcdef });
  const mesh = new Mesh(geometry, material); source.add(mesh);
  const before = Array.from(geometry.attributes.position.array);
  for (const style of ['comic', 'fantasy']) for (const base of ['feminine', 'masculine']) {
    const sword = createMakerSword(style, base, 'baguette_sword', source);
    const grip = sword.getObjectByName('MakerSwordGrip'); let copy; sword.traverse(object => { if (object instanceof Mesh) copy = object; });
    assert.equal(sword.name, 'CharacterMakerSword'); assert.equal(sword.userData.makerWeapon, 'baguette_sword');
    assert.deepEqual(grip.position.toArray(), MAKER_WEAPON_SOCKETS[style][base].point);
    assert.notEqual(copy.geometry, geometry); assert.notEqual(copy.material, material);
    assert.ok(Array.from(copy.geometry.attributes.position.array).every(Number.isFinite));
    disposeMakerWeapon(sword);
    assert.equal(sword.children.length, 1); assert.equal(source.children.length, 1);
    assert.deepEqual(Array.from(geometry.attributes.position.array), before);
  }
});

test('main-list visible length compensates native bone/profile scale and follows Maker size', () => {
  for (const baseScale of [1, 2]) for (const userScale of [.65, 1, 1.35]) {
    const outer = new Group(); outer.scale.setScalar(baseScale * userScale);
    const model = new Group(); model.scale.setScalar(.75); outer.add(model);
    const hand = new Group(); hand.name = 'R_Hand'; hand.scale.setScalar(1.5); model.add(hand);
    const source = new Group(); source.userData.registeredWeapon = { id: 'baguette_sword' };
    const a = new Group(), b = new Group(); b.position.y = .75; source.add(a, b);
    const sword = createMakerSword('fantasy', 'feminine', 'baguette_sword', source);
    hand.add(sword); fitMakerWeaponScale(sword, hand, model, baseScale); sword.scale.setScalar(1);
    outer.updateMatrixWorld(true);
    const [from, to] = sword.getObjectByName('MakerWeaponHandlePlacement').children[0].children;
    assert.ok(Math.abs(from.getWorldPosition(new Vector3()).distanceTo(to.getWorldPosition(new Vector3())) - .75 * sword.userData.makerWeaponFit.visualScale * userScale) < 1e-10);
  }
});

test('none owns no geometry and never falls back to a sword or emits enabled weapon tracks', () => {
  const none = createMakerSword('fantasy', 'feminine', 'none', null);
  assert.equal(none.getObjectByName('MakerSwordGrip').children.length, 0);
  assert.equal(none.userData.makerWeapon, 'none');
  assert.throws(() => createMakerSword('comic', 'masculine', 'baguette_sword', null));
  assert.throws(() => createMakerSword('comic', 'masculine', 'none', new Group()));
  const originals = ['idle', 'walk', 'run', 'sword_idle', 'sword_attack'].map(name => new AnimationClip(name, 1, []));
  const armed = addWeaponTracks(originals, true), unarmed = addWeaponTracks(originals, false);
  originals.forEach(clip => assert.equal(clip.tracks.length, 0));
  armed.forEach(clip => assert.deepEqual(Array.from(clip.tracks[0].values), Array(6).fill(clip.name.startsWith('sword_') ? 1 : 0)));
  unarmed.forEach(clip => assert.deepEqual(Array.from(clip.tracks[0].values), Array(6).fill(0)));
});

test('measured fits are attachment-only and expose nominal versus calibrated length', () => {
  const { makerWeaponFit } = require('../lib/character-maker/weapon-fit.ts');
  const source = new Group(); source.userData.registeredWeapon = { id: 'tamago_pan', length: .55 };
  const geometry = new BoxGeometry(.06, .55, .06), material = new MeshBasicMaterial(); source.add(new Mesh(geometry, material));
  const before = Array.from(geometry.attributes.position.array);
  const expected = [['generic-human-a', .7], ['generic-human-b', .7], ['fantasy-feminine', .65], ['fantasy-masculine', .65], ['comic-feminine', .75], ['comic-masculine', .75]];
  for (const [rig, scale] of expected) {
    const fit = makerWeaponFit('tamago_pan', rig);
    assert.equal(fit.sizeScale, scale); assert.deepEqual(fit.gripOffset, [-.015, -.025, -.02]);
    const sword = createMakerSword('fantasy', 'feminine', 'tamago_pan', source, rig);
    assert.deepEqual(sword.getObjectByName('MakerWeaponFit').scale.toArray(), [scale, scale, scale]);
    assert.deepEqual(sword.getObjectByName('MakerWeaponHandlePlacement').position.toArray(), fit.gripOffset);
    assert.equal(sword.userData.makerWeaponFit.nominalLength, .55);
    assert.equal(sword.userData.makerWeaponFit.calibratedLengthAtDefaultSize, .55 * fit.visualScale);
    assert.ok(fit.visualScale > 1.5);
    assert.equal(fit.effectiveScale, fit.visualScale);
    assert.deepEqual(sword.getObjectByName('MakerWeaponVisualScale').scale.toArray(), Array(3).fill(fit.visualScale));
    let mesh; sword.traverse(object => { if (object instanceof Mesh) mesh = object; });
    assert.notDeepEqual(Array.from(mesh.geometry.attributes.position.array), before);
    disposeMakerWeapon(sword);
  }
  for (const rig of ['generic-robot', 'generic-cat']) assert.equal(makerWeaponFit('tamago_pan', rig).sizeScale, 1);
  assert.equal(makerWeaponFit('baguette_sword', 'generic-human-a').sizeScale, 1);
  assert.deepEqual(Array.from(geometry.attributes.position.array), before);
});


test('main-list enlargement is anchored at the calibrated grip center for all rig families', () => {
  const { makerWeaponFit, MAKER_WEAPON_REFERENCE } = require('../lib/character-maker/weapon-fit.ts');
  assert.equal(MAKER_WEAPON_REFERENCE.mainListWeaponScale, 2);
  const cases = [
    ['generic-human-a', 'cake_sword'], ['generic-human-b', 'cake_sword'],
    ['generic-robot', 'cake_sword'], ['generic-cat', 'cake_sword'],
    ['fantasy-feminine', 'momo_hammer'], ['fantasy-masculine', 'tamago_pan'],
    ['comic-feminine', 'taiyaki_sword'], ['comic-masculine', 'cake_sword'],
  ];
  for (const [rig, id] of cases) {
    const fit = makerWeaponFit(id, rig), source = new Group();
    const selectedHandleCenter = new Group(); selectedHandleCenter.position.fromArray(fit.gripOffset).negate(); source.add(selectedHandleCenter);
    const sword = createMakerSword('fantasy', 'feminine', id, source, rig); sword.scale.setScalar(1); sword.updateMatrixWorld(true);
    const attachedCenter = sword.getObjectByName('MakerWeaponHandlePlacement').children[0].children[0];
    const grip = sword.getObjectByName('MakerSwordGrip');
    assert.ok(attachedCenter.getWorldPosition(new Vector3()).distanceTo(grip.getWorldPosition(new Vector3())) < 1e-10);
    assert.equal(sword.userData.makerWeaponFit.visualScale, fit.visualScale);
  }
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
