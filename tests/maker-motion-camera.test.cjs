// Run: node --test tests/maker-motion-camera.test.cjs
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const { test } = require('node:test');
const assert = require('node:assert/strict');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { requestWeaponForMotion } = require('../lib/character-maker/weapon-motion-intent.ts');
const { selectedMakerWeaponId } = require('../lib/character-maker/weapon-catalog.ts');
const { replaceEditPresent, createEditHistory } = require('../lib/character-maker/edit-history.ts');

// Execute the real editor handlers without a browser or WebGL.
function sourceNode(filename, predicate) {
  const source = ts.createSourceFile(filename, fs.readFileSync(path.join(__dirname, '../components/character-maker', filename), 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  let found;
  function visit(node) { if (!found && predicate(node)) found = node; else ts.forEachChild(node, visit); }
  visit(source);
  assert.ok(found, `Missing UI node in ${filename}`);
  return found.getText(source);
}
function run(source, context) {
  return vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
}
const editors = ['OriginalCharacterMaker.tsx', 'MakerPresetEditor.tsx'];
const handlers = Object.fromEntries(editors.map(filename => [filename, sourceNode(filename, node => ts.isFunctionDeclaration(node) && node.name?.text === 'chooseMotion')]));
function choose(filename, current, next, framing = 'full', weaponId = 'cake_sword', custom = true) {
  const result = { motion: current, framing, custom, cameraRevision: 7, attackRevision: 2, motionRevision: 3, playing: false, focused: 0, pending: null };
  const context = {
    framing, weaponId, motion: current, config: { motion: current }, next, requestWeaponForMotion,
    setPendingWeaponMotion: value => { result.pending = value; }, chosenWeaponMotion: { current: null },
    weaponPicker: { current: { focusChoices: () => { result.focused++; } } },
    change: value => Object.assign(result, value),
    setMotion: value => { result.motion = value; }, setPlaying: value => { result.playing = value; },
    setFraming: value => { result.framing = value; }, setCameraCustom: value => { result.custom = value; },
    setCameraRevision: update => { result.cameraRevision = update(result.cameraRevision); },
    setAttackRevision: update => { result.attackRevision = update(result.attackRevision); },
    setMotionRevision: update => { result.motionRevision = update(result.motionRevision); },
  };
  run(`${handlers[filename]}; chooseMotion(next);`, context);
  return result;
}

test('ready, attack and repeated attacks retain the current full-view camera in both editors', () => {
  for (const editor of editors) for (const current of ['sword_idle', 'sword_attack']) for (const next of ['sword_idle', 'sword_attack']) for (const custom of [false, true]) {
    const result = choose(editor, current, next, 'full', 'cake_sword', custom);
    assert.equal(result.cameraRevision, 7, `${editor}: ${current} -> ${next}`);
    assert.equal(result.custom, custom);
    assert.equal(result.framing, 'full');
    assert.equal(result.motion, next);
    assert.equal(result.playing, true);
    assert.equal(result.focused, 0);
    if (editor === 'OriginalCharacterMaker.tsx') assert.equal(result.attackRevision, next === 'sword_attack' ? 3 : 2);
    else assert.equal(result.motionRevision, 4);
  }
});

test('replaying any existing full-view motion preserves the camera', () => {
  for (const editor of editors) for (const motion of ['idle', 'walk', 'run', 'sword_idle', 'sword_attack']) {
    const result = choose(editor, motion, motion);
    assert.equal(result.cameraRevision, 7, `${editor}: ${motion}`);
    assert.equal(result.custom, true);
    assert.equal(result.playing, true);
  }
});

test('face-to-motion and unrelated motion choices still request their normal full framing', () => {
  for (const editor of editors) {
    for (const current of ['sword_idle', 'sword_attack']) for (const next of ['sword_idle', 'sword_attack']) {
      const result = choose(editor, current, next, 'face');
      assert.equal(result.cameraRevision, 8);
      assert.equal(result.framing, 'full');
      if (editor === 'OriginalCharacterMaker.tsx') assert.equal(result.custom, false);
    }
    for (const [current, next] of [['sword_idle', 'walk'], ['walk', 'sword_attack'], ['idle', 'run']]) {
      assert.equal(choose(editor, current, next).cameraRevision, 8, `${editor}: ${current} -> ${next}`);
    }
  }
});

test('unarmed ready/attack requests still ask for equipment before changing motion or camera', () => {
  for (const editor of editors) for (const next of ['sword_idle', 'sword_attack']) {
    const result = choose(editor, 'idle', next, 'face', 'none');
    assert.equal(result.pending, next);
    assert.equal(result.focused, 1);
    assert.equal(result.motion, 'idle');
    assert.equal(result.playing, false);
    assert.equal(result.framing, 'face');
    assert.equal(result.cameraRevision, 7);
    assert.equal(result.custom, true);
  }
});

test('automatic attack completion returns to ready without touching the camera', () => {
  const presetComplete = sourceNode('MakerPresetEditor.tsx', node => ts.isVariableDeclaration(node) && node.name.getText() === 'onMotionComplete');
  const originalComplete = sourceNode('OriginalCharacterMaker.tsx', node => ts.isVariableDeclaration(node) && node.name.getText() === 'onAttackComplete');
  const failCamera = () => assert.fail('Completion must not reset the camera');
  for (const weaponId of ['cake_sword', 'baguette_sword', 'none']) {
    let motion;
    run(`const ${presetComplete}; onMotionComplete();`, {
      useCallback: value => value, selectedMakerWeaponId, documentRef: { current: { config: { weaponId } } },
      setMotion: value => { motion = value; }, setCameraRevision: failCamera, setFraming: failCamera,
    });
    assert.equal(motion, weaponId === 'none' ? 'idle' : 'sword_idle');
  }
  const historyRef = { current: createEditHistory({ config: { motion: 'sword_attack', weaponId: 'cake_sword' } }) };
  let writes = 0;
  run(`const ${originalComplete}; onAttackComplete();`, {
    useCallback: value => value, historyRef, replaceEditPresent,
    installHistory: value => { historyRef.current = value; }, draftWriter: { changed: () => { writes++; } },
    setCameraRevision: failCamera, setFraming: failCamera, setCameraCustom: failCamera,
  });
  assert.equal(historyRef.current.present.config.motion, 'sword_idle');
  assert.equal(writes, 1);
  assert.equal(choose('OriginalCharacterMaker.tsx', historyRef.current.present.config.motion, 'sword_attack').cameraRevision, 7);
});

test('enlarged weapon framing reserves horizontal space without a motion-dependent camera', () => {
  const { makerArmedCameraDistance } = require('../lib/character-maker/armed-camera.ts');
  assert.equal(makerArmedCameraDistance(2, 1.4), 6.6);
  assert.equal(makerArmedCameraDistance(2, .8), 8.25);
  assert.equal(makerArmedCameraDistance(2, .5), 13.2);
});

test('the studio lens scales the armed distance by an explicit factor and leaves the classic default alone', () => {
  const { makerArmedCameraDistance } = require('../lib/character-maker/armed-camera.ts');
  assert.equal(makerArmedCameraDistance(2, 1.4, 1), makerArmedCameraDistance(2, 1.4));
  assert.ok(Math.abs(makerArmedCameraDistance(2, 1.4, .8) - 5.28) < 1e-9);
  assert.ok(Math.abs(makerArmedCameraDistance(2, .5, .8) - 10.56) < 1e-9);
});

test('the studio full fit plants the feet on the floor line and keeps a body-centred envelope inside the visible width', () => {
  const { Box3, Vector3 } = require('three');
  const { fitStageFull, projectBox, stageDirection, STAGE_FULL_LAYOUT } = require('../components/character-maker/stage-camera.ts');
  const direction = stageDirection(0);
  // A 1.6-tall body with a sword arc reaching far to one side.
  const envelope = new Box3(new Vector3(-1.4, 0, -.3), new Vector3(.3, 1.6, .3));
  const body = new Box3(new Vector3(-.3, 0, -.2), new Vector3(.3, 1.6, .2));
  for (const [width, height] of [[852, 752], [390, 334], [1200, 400]]) {
    for (const centre of [undefined, body]) {
      const fit = fitStageFull(envelope, direction, width, height, 1.02, centre);
      const { right, up, r0, r1, u0 } = projectBox(envelope, direction);
      const visibleH = height / fit.zoom, visibleW = width / fit.zoom;
      const bottomEdge = fit.target.dot(up) - visibleH / 2;
      // The soles of a character at the origin, toes 8% of the height toward the lens.
      const stand = centre ?? envelope;
      const soles = new Vector3(direction.x, 0, direction.z).normalize().multiplyScalar((stand.max.y - stand.min.y) * .08).setY(stand.min.y).dot(up);
      assert.ok(Math.abs((soles - bottomEdge) / visibleH - STAGE_FULL_LAYOUT.bottom) < 1e-9, `feet at ${STAGE_FULL_LAYOUT.bottom} for ${width}x${height}`);
      assert.ok(u0 >= bottomEdge, 'the envelope stays above the bottom edge');
      const left = fit.target.dot(right) - visibleW / 2, rightEdge = fit.target.dot(right) + visibleW / 2;
      assert.ok(r0 >= left - 1e-9 && r1 <= rightEdge + 1e-9, `envelope inside the visible width for ${width}x${height}`);
    }
  }
  // With room to spare, the body pulls the frame off the envelope centre.
  const wide = fitStageFull(envelope, direction, 1600, 400, 1.02, body);
  const { right } = projectBox(envelope, direction);
  assert.ok(wide.target.dot(right) > (envelope.min.x + envelope.max.x) / 2 * right.x + 1e-3);
});
