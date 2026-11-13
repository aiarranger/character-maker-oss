'use client';

// three を読まない（今の版と Rust の版の下見が使う）
/** Studio load failure (spec §5.5): one plain-Japanese board; raw URLs and English stay in console.warn. */
export function StageError({ onRetry }: { onRetry: () => void }) {
  return <div role="alert" data-stage-error="" style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: '96px 28px 28px', maxWidth: '100%', boxSizing: 'border-box', overflowWrap: 'anywhere', textAlign: 'center', background: 'color-mix(in oklab, var(--paper, #F7F3EC) 88%, transparent)', color: 'var(--ink, #17151C)', font: '600 var(--t-14, 14px)/1.5 var(--font-ui, system-ui, sans-serif)' }}>
    <strong style={{ font: '700 var(--t-16, 16px)/1.4 var(--font-ui, system-ui, sans-serif)' }}>キャラクターを読み込めませんでした</strong>
    <span>通信を確かめて、もう一度読み込んでください。</span>
    <button type="button" onClick={onRetry} style={{ minHeight: 44, maxWidth: '100%', boxSizing: 'border-box', padding: '10px 22px', borderRadius: 999, border: 0, background: 'var(--ink, #17151C)', color: '#fff', font: 'inherit', overflowWrap: 'anywhere', cursor: 'pointer', boxShadow: '0 2px 0 #000' }}>もう一度読み込む</button>
  </div>;
}

