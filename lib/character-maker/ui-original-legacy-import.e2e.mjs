// Import schema-4 saves of a human original through the real UI.
// node lib/character-maker/ui-original-legacy-import.e2e.mjs [output-directory]
//
// 2026-10 以降、人間のオリジナル（generic-human-a/b）は髪パーツ（modular）で組み立て、旧い髪型 ID は
// 読み込み時に今の髪パーツへ移す（original-config.ts の CURRENT_HUMAN_HAIR）。引退した appearance モデルは読まない。
// このため旧保存（schema 4）は「そのまま」ではなく、次の形に移って読み込まれることを確かめる。
//   schema 5 / original-maker-v2 / appearance v2（parts は既定）/ 髪型は移し先 / 体型・髪色・動作・表情はそのまま
// 旧画面の確認で /tmp に残っていた旧保存は無いので、カタログの値から schema 4 の保存を作る。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';
import { importPlaywright } from '../../tools/lib/paths.mjs';
import { ORIGINAL_DRAFT_KEY, STUDIO_URL, catalogEntry, current, exportFile, importJson, installReadTool, openStudio, ready as studioReady, selectOriginal, waitDocument } from '../../tools/lib/studio-e2e.mjs';

const out = process.argv[2] ?? '/tmp/maker-original-legacy-import';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const haru = catalogEntry('generic-human-a');
assert.ok(haru.modularModel && haru.appearanceModel, 'ハルは髪パーツと、引退した appearance モデルの両方をカタログに持つ');
const legacyDocument = (name, config) => ({
  format: 'characters.character-maker', sourceKind: 'original', schemaVersion: 4, assetVersion: 'original-maker-v1',
  name, characterId: haru.id, modelSha256: haru.modelSha256,
  config: { size: 1, face: { blinkLeft: 0, blinkRight: 0, mouthClose: 0, aa: 0, ee: 0, ou: 0, happy: 0 }, motion: 'idle', playing: true, speed: 1, autoBlink: true, speaking: false, hairMotion: false, headTurn: 0, ...config },
});
const defaultParts = { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: 'source', eyeColor: 'source' };
const fixtures = [
  // appearance の無い、最初期の保存。元の髪型（original）はハルの今のショート（haru-short-v2）へ移る。
  ['without-appearance', legacyDocument('ハル（旧保存）', {}), { build: 0, hairstyle: 'haru-short-v2', hairColor: 'source' }],
  // appearance v1 の保存（体型 +1・ショート・ローズ・歩く）。ショートは刈り上げショート（haru-crop-v2）へ移る。
  ['appearance-v1', legacyDocument('ハル・体型とショート', { motion: 'walk', appearance: { version: 'original-appearance-v1', build: 1, hairstyle: 'short', hairColor: '#b97991' } }),
    { build: 1, hairstyle: 'haru-crop-v2', hairColor: '#b97991' }],
];
await fs.mkdir(out, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const results = [], errors = [], failures = [];
try {
  for (const [id, input, migrated] of fixtures) {
    const inputPath = path.join(out, `${id}.input.character.json`);
    await fs.writeFile(inputPath, JSON.stringify(input, null, 2) + '\n');
    const expected = { ...structuredClone(input), schemaVersion: 5, assetVersion: 'original-maker-v2',
      config: { ...structuredClone(input.config), appearance: { version: 'original-appearance-v2', parts: defaultParts, ...migrated } } };
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    await installReadTool(context);
    const page = await context.newPage(); page.setDefaultTimeout(30000);
    const responses = []; let phase = 'startup';
    page.on('pageerror', e => errors.push({ id, message: e.message }));
    page.on('response', r => {
      if (new URL(r.url()).pathname.startsWith('/models/')) responses.push({ phase, url: new URL(r.url()).pathname, status: r.status() });
    });
    const ready = async () => {
      await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === '3Dモデル（GLB）' && !b.disabled)
        || document.querySelector('[aria-label="キャラクターの3Dプレビュー"] [role="alert"]'), null, { timeout: 120000 });
      const error = page.locator('[aria-label="キャラクターの3Dプレビュー"] [role="alert"]');
      if (await error.count()) throw new Error(await error.first().innerText());
      await studioReady(page);
    };
    try {
      console.log(JSON.stringify({ id, stage: 'loading-ui' }));
      await openStudio(page); await selectOriginal(page, haru.id); await ready();
      assert(responses.some(r => r.url === haru.modularModel.url && r.status === 200), 'ハルを選ぶと髪パーツ入りのモデルを読む');
      phase = 'legacy-import'; console.log(JSON.stringify({ id, stage: 'importing-fixture' }));
      await importJson(page, inputPath);
      await waitDocument(page, { name: input.name });
      await ready();
      assert.deepEqual(await current(page), expected);
      const stored = await page.evaluate(key => JSON.parse(sessionStorage.getItem(key)).document, ORIGINAL_DRAFT_KEY);
      assert.deepEqual(stored, expected, '読み込んだ内容がそのまま控えに残る');
      const outputPath = await exportFile(page, '編集設定（JSON）', path.join(out, id + '.character.json'));
      const outputBytes = await fs.readFile(outputPath), exported = JSON.parse(outputBytes); assert.deepEqual(exported, expected);
      assert(!responses.some(r => r.url === haru.appearanceModel.url), '引退した appearance モデルは読まない');
      // 画面が読んだ URL を、資格情報なしの別の GET で取り直してカタログの SHA と照らす。
      const verificationResponse = await fetch(new URL(haru.modularModel.url, STUDIO_URL), { signal: AbortSignal.timeout(120000) });
      assert.equal(verificationResponse.status, 200);
      const responseBytes = Buffer.from(await verificationResponse.arrayBuffer());
      assert.equal(sha(responseBytes), haru.modularModel.sha256);
      const canvas = await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas').toDataURL('image/png'))))));
      const canvasBytes = Buffer.from(canvas.split(',')[1], 'base64'); const raw = await sharp(canvasBytes).ensureAlpha().raw().toBuffer();
      let painted = 0; for (let i = 3; i < raw.length; i += 4) if (raw[i] > 240) painted++;
      assert(painted > 1000, 'Imported legacy character must be visibly rendered');
      await fs.writeFile(path.join(out, id + '-canvas.png'), canvasBytes); await page.evaluate(() => window.scrollTo(0, 0));
      await page.screenshot({ path: path.join(out, id + '.jpg'), type: 'jpeg', quality: 93 });
      results.push({ id, inputPath, inputSha256: sha(JSON.stringify(input, null, 2) + '\n'), schemaVersion: exported.schemaVersion, assetVersion: exported.assetVersion,
        normalizedMissingAppearance: !input.config.appearance, appearance: exported.config.appearance, migratedHair: migrated.hairstyle,
        modularModel: { url: haru.modularModel.url, expectedSha256: haru.modularModel.sha256, receivedSha256: sha(responseBytes), receivedBytes: responseBytes.length },
        retiredAppearanceModelRequested: false, outputPath, outputSha256: sha(outputBytes), paintedPixels: painted, responses, screenshot: path.join(out, id + '.jpg') });
      await fs.writeFile(path.join(out, 'progress.json'), JSON.stringify({ results, errors }, null, 2));
      console.log(JSON.stringify({ id, schemaVersion: exported.schemaVersion, appearance: exported.config.appearance, painted }));
    } catch (error) {
      await page.screenshot({ path: path.join(out, id + '-failure.jpg'), type: 'jpeg', quality: 93 }).catch(() => {});
      await fs.writeFile(path.join(out, id + '-failure.json'), JSON.stringify({ error: String(error), responses,
        document: await current(page).catch(() => null),
        visibleText: await page.locator('body').innerText().catch(() => '') }, null, 2));
      failures.push({ id, error: String(error) });
    } finally { await context.close(); }
  }
  assert.deepEqual(failures, []);
  assert.deepEqual(errors, []);
  await fs.writeFile(path.join(out, 'result.json'), JSON.stringify({ success: true, method: 'Two schema-4 JSON saves imported once each through the Studio UI in separate fresh Chrome contexts. No editing action. The migrated document is compared with the live editor, the tab draft and the re-exported JSON; the modular model URL is fetched independently for SHA verification.', results, errors }, null, 2));
} catch (error) { await fs.writeFile(path.join(out, 'failure.json'), JSON.stringify({ error: String(error), results, errors }, null, 2)); throw error; }
finally { await browser.close(); }
