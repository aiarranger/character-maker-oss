'use client';
import { useId, useImperativeHandle, useRef, useState, type CSSProperties, type KeyboardEvent, type Ref } from 'react';
import type { ArmedWeaponMotion } from '../../lib/character-maker/weapon-motion-intent';
import { MAKER_WEAPON_CHOICES, type MakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import styles from './character-studio.module.css';

export type WeaponPickerHandle = { focusChoices: () => void };

const CRUMBS = [0, 1, 2, 3, 4, 5].map(index => {
  const angle = (index / 6) * Math.PI * 2 - Math.PI / 2 + (index % 2 ? 0.35 : -0.2);
  const radius = 24 + (index * 7) % 13;
  return { x: Math.round(Math.cos(angle) * radius), y: Math.round(Math.sin(angle) * radius), size: 4 + (index % 3), color: ['var(--accent)', '#F4C27A', '#E8553A'][index % 3] };
});

/** 矢印キー・Home・End で隣の皿へ移る（ボタンのまま。ロービング tabindex）。 */
function moveFocusWithArrows(event: KeyboardEvent<HTMLElement>) {
  const forward = ['ArrowDown', 'ArrowRight'], backward = ['ArrowUp', 'ArrowLeft'];
  if (![...forward, ...backward, 'Home', 'End'].includes(event.key)) return;
  const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
  if (!buttons.length) return;
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
  const index = current < 0 ? Math.max(0, buttons.findIndex(button => button.getAttribute('aria-pressed') === 'true')) : current;
  event.preventDefault();
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1
    : current < 0 ? index : (index + (forward.includes(event.key) ? 1 : -1) + buttons.length) % buttons.length;
  buttons[next].focus();
  buttons[next].scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

/** Every registered choice is visible and takes one click, in both editor modes (§5.4 のトレイ). */
export function WeaponPicker({ value, onChange, disabled = false, pickerRef, requestedMotion, onCancelRequest }: {
  value: MakerWeaponId; onChange: (value: MakerWeaponId) => void; disabled?: boolean;
  pickerRef?: Ref<WeaponPickerHandle>; requestedMotion?: ArmedWeaponMotion | null; onCancelRequest?: () => void;
}) {
  const id = useId(), choices = useRef<HTMLDivElement>(null);
  // M3: 選んだ皿だけが弾み、パン屑が飛ぶ。最初の表示では出さない。
  const [burst, setBurst] = useState<{ id: MakerWeaponId; key: number } | null>(null);
  useImperativeHandle(pickerRef, () => ({ focusChoices() {
    const element = choices.current;
    if (!element || disabled) return;
    element.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    element.focus({ preventScroll: true });
  } }));
  const selected = MAKER_WEAPON_CHOICES.find(item => item.id === value) ?? MAKER_WEAPON_CHOICES[0];
  return <section className={styles.weaponBar} aria-label="持たせる武器" data-awaiting-weapon={Boolean(requestedMotion)}>
    <div className={styles.weaponHeading}>
      <strong id={`${id}-label`}>武器</strong>
      <small id={`${id}-help`} role={requestedMotion ? 'status' : undefined} aria-live="polite">{requestedMotion
        ? `使う武器をクリックすると「${requestedMotion === 'sword_attack' ? '武器を振る' : '武器を構える'}」を始めます。`
        : selected.name}</small>
      {requestedMotion && <button type="button" className={styles.textButton} onClick={onCancelRequest}>キャンセル</button>}
    </div>
    <div ref={choices} className={styles.weaponChoices} role="group" aria-labelledby={`${id}-label`} aria-describedby={`${id}-help`} tabIndex={-1} data-awaiting-weapon={Boolean(requestedMotion)} onKeyDown={moveFocusWithArrows}>
      {MAKER_WEAPON_CHOICES.map((item, index) => <button key={item.id} type="button" className={styles.weaponChoice} aria-label={item.name} title={item.name} aria-pressed={value === item.id} disabled={disabled} tabIndex={value === item.id ? 0 : -1}
        style={{ '--i': index } as CSSProperties}
        onClick={() => { setBurst(previous => ({ id: item.id, key: (previous?.key ?? 0) + 1 })); onChange(item.id); }}>
        <span className={styles.weaponPlate} key={burst?.id === item.id ? burst.key : 0} data-burst={burst?.id === item.id ? 'true' : undefined}>
          {item.id === 'none' ? <span className={styles.weaponNone} aria-hidden="true" />
            : <img src={`/review-swing-2026-10-02/weapons/${item.id}.png`} alt="" width={44} height={44} decoding="async" draggable={false} />}
        </span>
        {burst?.id === item.id && <span key={`crumbs-${burst.key}`} className={styles.crumbs} aria-hidden="true">{CRUMBS.map((crumb, index) => <i key={index} style={{ '--x': `${crumb.x}px`, '--y': `${crumb.y}px`, '--s': `${crumb.size}px`, '--c': crumb.color } as CSSProperties} />)}</span>}
        {value === item.id && <span className={styles.weaponCheck} aria-hidden="true"><svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"><path d="m3.5 8.5 3 3 6-7" /></svg></span>}
      </button>)}
    </div>
  </section>;
}
