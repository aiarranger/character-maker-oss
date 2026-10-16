import type { MakerWeaponId } from './weapon-catalog';

export type MakerWeaponRig = 'generic-human-a' | 'generic-human-b' | 'generic-robot' | 'generic-cat'
  | 'fantasy-feminine' | 'fantasy-masculine' | 'comic-feminine' | 'comic-masculine';
type GripFit = { sizeScale: number; gripOffset: readonly [number, number, number] };
export const MAKER_WEAPON_FIT_VERSION = 'registered-maker-protected-grip-size-2026-10-06-v2';

// The saved Aichan/Kiyogon main-list recipe uses body display scale 2 and
// weaponScale 2: cake length is about 71–73% of the displayed body height.
// A 2.5% silhouette allowance compensates for the protected, unscaled pommel.
// Match that proportion, measured in armed idle, rather than doubling the old
// whole-weapon hand fits. Character height edits scale the result as before.
export const MAKER_WEAPON_REFERENCE = { mainListWeaponScale: 2, cakeToBodyHeight: 0.72, cakeLength: 0.7, protectedGripAllowance: 1.025 } as const;
const RIG_BODY_HEIGHT: Record<MakerWeaponRig, number> = {
  'generic-human-a': 1.881642, 'generic-human-b': 1.801445,
  'generic-robot': 1.977154, 'generic-cat': 1.918833,
  'fantasy-feminine': 1.585314, 'fantasy-masculine': 1.682219,
  'comic-feminine': 1.568705, 'comic-masculine': 1.607421,
};
export const MAKER_WEAPON_GRIP_PROTECTION = { radius: 0.045, transition: 0.085 } as const;

// Measured, visual-checked attachment transforms only. Nominal source length,
// registered source assets, retained socket frames and hand geometry stay unchanged.
// Instance weapon geometry receives the documented protected-grip size transform.
const FITS: Partial<Record<MakerWeaponRig, Partial<Record<MakerWeaponId, GripFit>>>> = {
  "generic-human-a": {
    "tamago_pan": {
      "sizeScale": 0.7,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "cake_sword": {
      "sizeScale": 0.8,
      "gripOffset": [
        -0.005,
        -0.06,
        -0.0045
      ]
    }
  },
  "generic-human-b": {
    "tamago_pan": {
      "sizeScale": 0.7,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "cake_sword": {
      "sizeScale": 0.8,
      "gripOffset": [
        -0.005,
        -0.06,
        -0.0045
      ]
    }
  },
  "generic-robot": {},
  "generic-cat": {},
  "fantasy-feminine": {
    "tamago_pan": {
      "sizeScale": 0.65,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "momo_hammer": {
      "sizeScale": 0.7,
      "gripOffset": [
        -0.0015,
        -0.035,
        0.0005
      ]
    }
  },
  "fantasy-masculine": {
    "tamago_pan": {
      "sizeScale": 0.65,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "momo_hammer": {
      "sizeScale": 0.7,
      "gripOffset": [
        -0.0015,
        -0.035,
        0.0005
      ]
    }
  },
  "comic-feminine": {
    "tamago_pan": {
      "sizeScale": 0.75,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "taiyaki_sword": {
      "sizeScale": 0.75,
      "gripOffset": [
        0.0095,
        -0.05,
        -0.0002
      ]
    }
  },
  "comic-masculine": {
    "tamago_pan": {
      "sizeScale": 0.75,
      "gripOffset": [
        -0.015,
        -0.025,
        -0.02
      ]
    },
    "taiyaki_sword": {
      "sizeScale": 0.75,
      "gripOffset": [
        0.0095,
        -0.05,
        -0.0002
      ]
    }
  }
};
const SOURCE_FIT: GripFit = { sizeScale: 1, gripOffset: [0, 0, 0] };
export function makerWeaponFit(weaponId: MakerWeaponId, rig: MakerWeaponRig) {
  const fit = FITS[rig]?.[weaponId] ?? SOURCE_FIT;
  const visualScale = RIG_BODY_HEIGHT[rig] * MAKER_WEAPON_REFERENCE.cakeToBodyHeight / MAKER_WEAPON_REFERENCE.cakeLength * MAKER_WEAPON_REFERENCE.protectedGripAllowance;
  return { version: MAKER_WEAPON_FIT_VERSION, rig, ...fit, visualScale, effectiveScale: visualScale,
    geometryTransform: 'protected-grip-uniform-enlargement-v1', gripProtection: MAKER_WEAPON_GRIP_PROTECTION,
    reference: MAKER_WEAPON_REFERENCE, referenceBodyHeight: RIG_BODY_HEIGHT[rig] };
}

/** Compensate the outer visual scale at the fist, then blend to the full design. */
export function makerWeaponGripCompensation(gripLocalY: number, fit: ReturnType<typeof makerWeaponFit>) {
  const { radius, transition } = fit.gripProtection;
  const t = Math.max(0, Math.min(1, (Math.abs(gripLocalY) - radius) / transition));
  const smooth = t * t * (3 - 2 * t);
  return (1 - smooth) / fit.visualScale + smooth / fit.sizeScale;
}
