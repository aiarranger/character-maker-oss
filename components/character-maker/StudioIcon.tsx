// Character Studio の線画アイコン（docs/studio-quality-spec.md §4.4）。
// 20px・stroke 1.75・丸い端と角・currentColor。<title> は入れない（ボタンの textContent を変えないため）。

export type StudioIconName =
  | 'undo' | 'redo' | 'reset' | 'play' | 'pause' | 'download' | 'more' | 'cards' | 'pencil' | 'plus' | 'close'
  | 'idle' | 'walk' | 'run' | 'guard' | 'swing' | 'orbit' | 'pinch' | 'check' | 'hit' | 'hurtFace';

export function StudioIcon({ name, size = 20, className }: { name: StudioIconName; size?: number; className?: string }) {
  return <svg className={className} width={size} height={size} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {name === 'undo' && <><path d="M7.5 5 4 8.5 7.5 12" /><path d="M4.5 8.5h7a4 4 0 0 1 0 8H9" /></>}
    {name === 'redo' && <><path d="M12.5 5 16 8.5 12.5 12" /><path d="M15.5 8.5h-7a4 4 0 0 0 0 8H11" /></>}
    {name === 'reset' && <><path d="M4 10a6 6 0 1 0 1.8-4.3" /><path d="M4 3.5v3.2h3.2" /></>}
    {name === 'play' && <path d="M7 4.8v10.4a.6.6 0 0 0 .9.5l8-5.2a.6.6 0 0 0 0-1L7.9 4.3a.6.6 0 0 0-.9.5Z" fill="currentColor" stroke="none" />}
    {name === 'pause' && <><rect x="5.5" y="4.5" width="3" height="11" rx="1" fill="currentColor" stroke="none" /><rect x="11.5" y="4.5" width="3" height="11" rx="1" fill="currentColor" stroke="none" /></>}
    {name === 'download' && <path d="m5.5 8 4.5 4.5L14.5 8" />}
    {name === 'more' && <><circle cx="5" cy="10" r="1.2" fill="currentColor" stroke="none" /><circle cx="10" cy="10" r="1.2" fill="currentColor" stroke="none" /><circle cx="15" cy="10" r="1.2" fill="currentColor" stroke="none" /></>}
    {name === 'cards' && <><rect x="3" y="5.5" width="9.5" height="11.5" rx="2" /><path d="M7 3h8a2 2 0 0 1 2 2v9" /></>}
    {name === 'pencil' && <><path d="M12.8 3.8a1.8 1.8 0 0 1 2.6 2.6L7 14.8l-3.3.7.7-3.3Z" /><path d="m11.5 5.2 2.6 2.6" /></>}
    {name === 'plus' && <path d="M10 4.5v11M4.5 10h11" />}
    {name === 'close' && <path d="m5.5 5.5 9 9m0-9-9 9" />}
    {name === 'check' && <path d="m4.5 10.5 3.5 3.5 7.5-8" />}
    {name === 'idle' && <><circle cx="10" cy="3.8" r="1.8" /><path d="M10 6.5v6.5M6.5 8.5 10 7l3.5 1.5M8 17.5l2-4.5 2 4.5" /></>}
    {name === 'walk' && <><circle cx="11" cy="3.8" r="1.8" /><path d="m10.5 6.5-1.5 6 -2.5 4.5M9.5 9 7 10.5M10.5 7l3 3 2 .5M9 12.5l3 1.5 1 3.5" /></>}
    {name === 'run' && <><circle cx="12.5" cy="3.6" r="1.8" /><path d="m6 8 3.5-1.8 3.5 2.3 2.5.3M9.5 6.2l-1.8 5.3L4 15M7.7 11.5l4.3 1.6.8 4.4M2.5 8.5h2M2 11.5h1.8" /></>}
    {name === 'guard' && <><path d="M10 2.8 15.5 5v4.5c0 3.6-2.4 6.2-5.5 7.7-3.1-1.5-5.5-4.1-5.5-7.7V5Z" /><path d="M10 6v7" /></>}
    {name === 'swing' && <><path d="M4 16 13.5 6.5" /><path d="m12 5 3-1.5-1.5 3" /><path d="M5.5 4.5a8 8 0 0 1 10 10" strokeDasharray="1.5 2.5" /></>}
    {name === 'orbit' && <><ellipse cx="10" cy="10" rx="7.5" ry="3.2" /><path d="m15 5.5 2.2 1.8-2.6 1" /><circle cx="10" cy="10" r="1.6" fill="currentColor" stroke="none" /></>}
    {name === 'hit' && <><circle cx="8" cy="4" r="1.8" /><path d="M8.5 6.5 7 12l-2.5 5M7.5 9 4 8M8 9.5l3.5 1M7 12l3 1.5 .5 4" /><path d="m13.5 4 1.5 1.5M16.5 6.5 15 7.5M14.5 2.5l.2 1.5" /></>}
    {name === 'hurtFace' && <><circle cx="10" cy="10" r="7" /><path d="m5.8 7.6 2.6 1.4-2.6 1.4M14.2 7.6 11.6 9l2.6 1.4" /><path d="M8 13.4q2-1.2 4 0" /></>}
    {name === 'pinch' && <><path d="M4 4l4 4M4 4v3M4 4h3M16 16l-4-4M16 16v-3M16 16h-3" /></>}
  </svg>;
}

/** ブランドのマーク（§4.5）。角丸の墨の四角に、右が開いた白い C の輪と、今のキャラの色の点。 */
export function StudioMark({ size = 28 }: { size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 28 28" aria-hidden="true" focusable="false">
    <rect width="28" height="28" rx="7" fill="#17151C" />
    <path d="M18.6 9.4a6.5 6.5 0 1 0 0 9.2" fill="none" stroke="#fff" strokeWidth="3.5" strokeLinecap="round" />
    <circle cx="20.6" cy="14" r="2.3" fill="var(--accent, #B0512A)" style={{ transition: 'fill var(--d-slow, 480ms) ease' }} />
  </svg>;
}
