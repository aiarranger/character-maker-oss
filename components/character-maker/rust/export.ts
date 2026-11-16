// Rust の Stage での書き出し（今の版は lib/character-maker/assembly.ts の exportCharacterGlb と yukikau-package.ts の exportPresetYukikauPackage）。
// 今の版と同じく、見せている下見とは別に組み立てた 1 体から書く（見えない canvas の Stage）。three は読まない。
import type { CharacterConfig } from '../../../lib/character-maker/config';
import { selectedMakerWeaponId } from '../../../lib/character-maker/weapon-catalog';
import { packageMeasured } from '../../../lib/character-maker/yukikau-package';
import { createMakerStage, type MakerStage } from './stage';
import { presetSpec } from './RustMakerScene';
import { originalModelAsset, resolveOriginalCharacter, validateOriginalDocument, type OriginalCharacter, type OriginalDocument } from '../../../lib/character-maker/original-config';
import { originalSwordMetadata } from '../../../lib/character-maker/original-weapon-metadata';
import { fetchModel, originalSpec, originalSwordExtras } from './original-spec';

/** 書き出し用の Stage（見えない canvas。終わったら手放す） */
export async function withExportStage<T>(work: (stage: MakerStage) => Promise<T>): Promise<T> {
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const stage = await createMakerStage(canvas, { dpr: 1 });
  try { return await work(stage); } finally { stage.free(); }
}

/** プリセットの GLB（とその高さ） */
export async function exportPresetGlbRust(config: CharacterConfig, studio = false): Promise<{ bytes: Uint8Array; height: number }> {
  return withExportStage(async stage => {
    const spec = await presetSpec(stage, config, studio);
    stage.assemble(JSON.stringify(spec));
    const bytes = stage.export('{}');
    return { bytes, height: stage.glb_height(bytes) };
  });
}

/** yukikau.jp 用（.character.zip）。武器を外した通常の GLB（選んだ武器は character.json の weapon） */
export async function exportPresetYukikauPackageRust(name: string, config: CharacterConfig, canvas?: HTMLCanvasElement | null, studio = false): Promise<File> {
  const { bytes, height } = await exportPresetGlbRust({ ...config, weaponId: 'none' }, studio);
  return packageMeasured(name, bytes, height, canvas, 'preset', selectedMakerWeaponId(config));
}

/** オリジナル 4 体の GLB（今の版の exportOriginalCharacterGlb。variant を渡さなければ動作で武器用・通常用を選ぶ）。
 * 'game' はゲーム用（魚群繚乱）: 通常用の形（武器も握った手も無い）に、待機・歩く・走ると構える・振るの両方を入れる。
 * Rust の版は形を常に 32bit 浮動小数で書く（KHR_mesh_quantization を使わない） */
export async function exportOriginalGlbRust(character: OriginalCharacter, source: OriginalDocument, options: { variant?: 'standard' | 'armed' | 'game' } = {}): Promise<{ bytes: Uint8Array; height: number }> {
  const document = validateOriginalDocument(source);
  resolveOriginalCharacter(document, [character]);
  const weaponId = selectedMakerWeaponId(document.config);
  const variant = options.variant ?? (['sword_idle', 'sword_attack'].includes(document.config.motion) ? 'armed' : 'standard');
  const asset = originalModelAsset(character, document.config.appearance);
  const model = await fetchModel(asset.url);
  const digest = await crypto.subtle.digest('SHA-256', model.slice().buffer);
  const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  if (sha256 !== asset.sha256) throw new Error('キャラクターのモデルが保存時の内容と一致しません。');
  const armed = variant === 'armed';
  const extras = {
    ...JSON.parse(JSON.stringify(document)),
    export: {
      pose: 'source-rest',
      face: 'configured-static-values',
      autoBlinkAndSpeaking: 'settings-only-not-baked',
      headTurn: 'applied-to-static-pose-original-animation-tracks-take-precedence',
      variant,
      motionScope: armed ? 'sword-idle-and-attack-plus-hit' : variant === 'game' ? 'idle-walk-run-and-sword-idle-attack-plus-hit' : 'idle-walk-and-run-plus-hit',
      clips: [] as string[],
      reactions: { hit: { clip: 'hit', loop: 'once', hurtFace: true }, hurtFace: { clip: 'face_hurt', node: 'maker-hurt-face', switch: 'node-scale-0-hidden-1-shown' } },
      weapon: armed ? originalSwordMetadata(character.id, weaponId) : { included: false, selectedId: weaponId, reason: 'standard-variant-excludes-weapon-and-closed-hand' },
      sourceProvenance: { modelSha256: asset.sha256, originalModular: null, originalModularPlacement: 'copied-from-source-model-top-level-extras-to-export-root-node-extras; not current editor state' },
    },
  };
  return withExportStage(async stage => {
    // 通常用は武器も握った手も付けずに組み立てる（今の版は attachOriginalSword を呼ばない）
    const config = armed ? document.config : { ...document.config, weaponId: 'none' as const };
    stage.assemble(JSON.stringify(await originalSpec(stage, character, config)));
    const bytes = stage.export(JSON.stringify({ variant, extras, weaponId, motions: character.motions, swordExtras: armed ? originalSwordExtras(character.id, weaponId) : null }));
    if (bytes.byteLength > 25 * 1024 * 1024) throw new Error('書き出したGLBが25MiBを超えています。');
    return { bytes, height: stage.glb_height(bytes) };
  });
}

/** yukikau.jp 用（.character.zip）。常に通常用（待機・歩く・走る＋hit・face_hurt）。選んだ武器は character.json の weapon */
export async function exportOriginalYukikauPackageRust(character: OriginalCharacter, document: OriginalDocument, canvas?: HTMLCanvasElement | null): Promise<File> {
  const clean = validateOriginalDocument(document);
  const { bytes, height } = await exportOriginalGlbRust(character, clean, { variant: 'standard' });
  return packageMeasured(clean.name, bytes, height, canvas, 'original', selectedMakerWeaponId(clean.config));
}

// ---- 魚群繚乱用（docs/gyogun-spec.md の「Studio で作ったキャラで遊ぶ」） ----
// 形は yukikau.jp 用と同じ .character.zip（character.json・model.glb・thumbnail.png。武器は character.json の weapon、GLB に武器の形は無い）。
// GLB に、待機・歩く・走るに加えて 構える・振る（sword_idle・sword_attack）と jump・dodge・skill・hit・face_hurt が入っていること。

/** プリセット: yukikau.jp 用と同じ GLB（武器を外した書き出しに、構える・振るとゲーム用の動作が全部入っている） */
export function exportPresetGyogunPackageRust(name: string, config: CharacterConfig, canvas?: HTMLCanvasElement | null, studio = false): Promise<File> {
  return exportPresetYukikauPackageRust(name, config, canvas, studio);
}

/** オリジナル 4 体: ゲーム用の GLB（yukikau.jp 用の通常用には構える・振るが無い） */
export async function exportOriginalGyogunPackageRust(character: OriginalCharacter, document: OriginalDocument, canvas?: HTMLCanvasElement | null): Promise<File> {
  const clean = validateOriginalDocument(document);
  const { bytes, height } = await exportOriginalGlbRust(character, clean, { variant: 'game' });
  return packageMeasured(clean.name, bytes, height, canvas, 'original', selectedMakerWeaponId(clean.config));
}
