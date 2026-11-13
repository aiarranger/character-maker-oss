'use client';

// WebGPU の無い端末で出す表示（aisns の /play と同じ文言。親の決定 2026-11-13）。ボタンは持たない（呼ぶ側が戻る口を渡したときだけ出す）。
export type StageExit = { onExit?: () => void; exitLabel?: string };

export function StageUnsupported({ onExit, exitLabel }: StageExit) {
  return <div role="alert" data-stage-unsupported="" style={{ display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, minHeight: '100%', padding: 28, boxSizing: 'border-box', textAlign: 'center', overflowWrap: 'anywhere', background: 'var(--paper, #F7F3EC)', color: 'var(--ink, #17151C)', font: '600 var(--t-14, 14px)/1.6 var(--font-ui, system-ui, sans-serif)' }}>
    <strong style={{ font: '700 var(--t-18, 18px)/1.4 var(--font-ui, system-ui, sans-serif)' }}>この端末では 3D を開けません</strong>
    <span style={{ maxWidth: '36em' }}>このブラウザは WebGPU に対応していません。Chrome・Edge の最新版か、iOS 26 以降の Safari で開いてください。</span>
    {onExit && <button type="button" onClick={onExit} style={{ minHeight: 44, padding: '10px 22px', borderRadius: 999, border: 0, background: 'var(--ink, #17151C)', color: '#fff', font: 'inherit', cursor: 'pointer' }}>{exitLabel ?? '戻る'}</button>}
  </div>;
}
