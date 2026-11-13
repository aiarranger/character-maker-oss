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
export async function exportPresetGlbRust(config: CharacterConfig): Promise<{ bytes: Uint8Array; height: number }> {
  return withExportStage(async stage => {
    const spec = await presetSpec(stage, config);
    stage.assemble(JSON.stringify(spec));
    const bytes = stage.export('{}');
    return { bytes, height: stage.glb_height(bytes) };
  });
}

/** yukikau.jp 用（.character.zip）。武器を外した通常の GLB（選んだ武器は character.json の weapon） */
export async function exportPresetYukikauPackageRust(name: string, config: CharacterConfig, canvas?: HTMLCanvasElement | null): Promise<File> {
  const { bytes, height } = await exportPresetGlbRust({ ...config, weaponId: 'none' });
  return packageMeasured(name, bytes, height, canvas, 'preset', selectedMakerWeaponId(config));
}

/** オリジナル 4 体の GLB（今の版の exportOriginalCharacterGlb。variant を渡さなければ動作で武器用・通常用を選ぶ）。
 * Rust の版は形を常に 32bit 浮動小数で書く（KHR_mesh_quantization を使わない） */
export async function exportOriginalGlbRust(character: OriginalCharacter, source: OriginalDocument, options: { variant?: 'standard' | 'armed' } = {}): Promise<{ bytes: Uint8Array; height: number }> {
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
      motionScope: armed ? 'sword-idle-and-attack-plus-hit' : 'idle-walk-and-run-plus-hit',
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
