# Character Studio (character-maker)

[日本語](README.md) | **English**

A browser-based 3D character creator. Pick a character, customise it on the spot, and export a GLB that you can drop straight into a game (idle / walk / run / sword / **hit reaction** / **＞＜ face**).

![Character Studio](docs/media/studio-hero.png)

![Flow: pick a character → hairstyle and hair colour → weapon → ready and swing the weapon → hit reaction (＞＜) → switch to Selene → export](docs/media/demo.gif)

[Demo video (MP4, 32 s)](docs/media/demo.mp4)

- 16 characters: 4 originals (Haru, Nami, Lux, Moka) + 8 fantasy + 4 comic-style
- Edit body shape, hairstyle, hair colour, face, eyes, mouth, expression, outfit colour and weapon (12 food-themed weapons)
- Motions: idle, walk, run, weapon ready stance, weapon swing, **hit reaction**; face: **＞＜** (squeezed eyes)
- Save (in the browser), export/import JSON, export GLB
- Next.js 15 / React 19 / three.js / React Three Fiber. No server or account needed

The UI is in Japanese. UI labels are given below as「Japanese」(English).

## Screens

| Fantasy preset Selene | Phone (390 px) |
|---|---|
| ![Selene](docs/media/studio-fantasy-selene.png) | <img src="docs/media/studio-phone.png" alt="Phone" width="300"> |

**＞＜ face** (top: the 4 originals, bottom: presets)

![＞＜ face](docs/media/hurt-face-grid.png)

**Hit reaction** (① idle ② recoil + ＞＜ ③ stagger ④ recover)

![Hit reaction](docs/media/hit-sequence.png)

**An exported GLB played in plain three.js** (idle, run, hit, hit face close-up)

![three.js](docs/media/glb-threejs.png)

## Quick start

Requires Node.js ≥ 22 (24 recommended) and pnpm 11.

```sh
pnpm install --frozen-lockfile
pnpm assets:sync      # builds the runtime assets in public/ from parts/ and distribution/ (binaries in public/ are not in git)
pnpm dev
```

Open http://127.0.0.1:4284/character-maker. The dev server listens on `127.0.0.1:4284` (to open it from a phone on the same LAN, run `pnpm exec next dev --hostname 0.0.0.0 --port 4284`).

| URL | Screen |
|---|---|
| `/character-maker` | Character Studio (pick one of 16 characters and customise it) |
| `/character-maker/classic` | The classic maker screen (standalone view of the embeddable `CharacterMaker` component) |

## Play on yukikau.jp

You can play as the characters you make in the game [yukikau.jp](https://yukikau.jp/).

1. **Download and import**: choose「書き出し」(Export) →「yukikau.jp 用（.character.zip）」(for yukikau.jp) to save the file, then click the「yukikau.jp で読み込む」(Import on yukikau.jp) link that appears (`https://yukikau.jp/play?import=1`) and select that file. The ZIP uses yukikau.jp's import format (`character.json`, `model.glb`, `thumbnail.png`) and includes the facing direction (+Z), height, idle/walk/run, the hit reaction and ＞＜.
2. **Make it inside yukikau.jp**: the「キャラメイク」(Character maker) entry on yukikau.jp (`/play?make=1`) opens this same Character Studio. Press「この姿で遊ぶ」(Play as this character) to start the game with the character you made.

## Use in your game

Use「書き出し」(Export, top right) →「3Dモデル（GLB）」(3D model) or「編集設定（JSON）」(editing settings) to save the files. The GLB loads as is in three.js, Babylon.js, Unity (glTFast) and other glTF loaders. Materials are unlit (`KHR_materials_unlit`).

### Animation clips

| Clip | What it is | How to play it |
|---|---|---|
| `idle` / `walk` / `run` | Idle, walk, run (standard export). The 4 originals use prefixed names such as `preset:biped:idle` | Loop |
| `sword_idle` / `sword_attack` | Weapon ready stance and swing (export with a weapon) | The stance loops; play the swing once, then return to `sword_idle` |
| `hit` | Hit reaction (recoil). The face turns ＞＜ while it plays | Play once (about 0.66 s), then return to the previous motion |
| `face_hurt` | The ＞＜ face only (does not move the body) | Loop it on top of another clip |

To show the ＞＜ face, play `face_hurt` **layered** on the current motion (`hit` already includes it). Depending on the character type, the face is shown either by scaling the face plate (node `maker-hurt-face`) or by morph weights, so use the clip rather than toggling the node yourself. The names are defined in `lib/character-maker/hurt.ts`; see [docs/main-character-spec.md](docs/main-character-spec.md) (Japanese) for details.

### three.js example

```js
import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';

const gltf = await new GLTFLoader().loadAsync('/my-character.glb');
scene.add(gltf.scene);
const mixer = new THREE.AnimationMixer(gltf.scene);
// The 4 originals' standard export uses prefixed names such as `preset:biped:idle`, so match by suffix
const clip = name => gltf.animations.find(c => c.name === name || c.name.endsWith(`:${name}`));

const idle = mixer.clipAction(clip('idle') ?? clip('sword_idle')).play();

// When hit: recoil with ＞＜, then return to the previous motion
function onDamaged() {
  const hit = mixer.clipAction(clip('hit'));
  hit.reset().setLoop(THREE.LoopOnce, 1).play();
  idle.crossFadeTo(hit, 0.08, false);
  mixer.addEventListener('finished', function back(event) {
    if (event.action !== hit) return;
    mixer.removeEventListener('finished', back);
    hit.crossFadeTo(idle.reset().play(), 0.15, false);
  });
}

// Show only the ＞＜ face for a while (the body keeps moving)
function showHurtFace(seconds = 1) {
  const face = mixer.clipAction(clip('face_hurt')).reset().play();
  setTimeout(() => face.stop(), seconds * 1000);
}

// Every frame
const clock = new THREE.Clock();
renderer.setAnimationLoop(() => { mixer.update(clock.getDelta()); renderer.render(scene, camera); });
```

### Embed as a React component

The Studio (16 characters, with hit reaction and ＞＜) can be embedded in another Next.js app as `CharacterStudio`. When you pass `onPlay`, each editor shows「この姿で遊ぶ」(Play as this character), and you receive the yukikau.jp-format `.character.zip` as a `File`. The Studio fills the viewport (`100dvh`) and does not need this repository's `globals.css` or layout (the host page decides where it goes).

```tsx
// next.config: transpilePackages: ['@aiarranger/character-maker']
import CharacterStudio from '@aiarranger/character-maker/CharacterStudio';

<CharacterStudio onPlay={async file => startGame(file)} />
```

Props: `onPlay?: (file: File) => void | Promise<void>`, `initialName?: string` (initial name for presets), `className?: string`.

The classic `CharacterMaker` (`components/character-maker/CharacterMaker.tsx`) can also be used as is. `onPlay` receives the exported GLB as a `File`.

```tsx
<CharacterMaker originals={[]} initialName="Hero" onPlay={async file => startGame(await file.arrayBuffer())} />
```

Serve the assets with `@aiarranger/character-maker/server`. Build the tarball for vendoring into your app with `pnpm pack:vendor` (`tools/pack.mjs`, kept under 50 MiB).

- `serveMakerAsset(req, res)`: serves `/maker-assets/…`, `/character-maker/…`, `/portraits/…`, `/review-swing-2026-10-02/…`, `/api/character-maker/assets/…` and `/models/…` (for a dev server). Paths it does not own (for example your own `/models/…` files) are left to the host.
- `await ensureStudioModels(cacheDir)`: the 4 originals' display models (`/models/`, about 97 MB) are not in the tarball. They are downloaded from a pinned commit of the public repository, checked with SHA-256 and stored in `cacheDir` (only missing or mismatched files). The list is `studio-models.json` in the package. `serveMakerAsset` also fetches a model on demand if it is not cached yet.
- `copyMakerAssets(outRoot)`: for static export. Copies the assets and the models prepared by `ensureStudioModels` into `outRoot`.

```js
import { ensureStudioModels, serveMakerAsset, copyMakerAssets } from '@aiarranger/character-maker/server';
await ensureStudioModels('.cache/character-maker-models');
// dev server: if (serveMakerAsset(req, res)) return;
// static export: copyMakerAssets('out');
```

## Project layout

| Path | Contents |
|---|---|
| `app/character-maker/` | Screens (Studio and classic) |
| `components/character-maker/`, `lib/character-maker/` | Editing, 3D view, saving, export |
| `parts/` | Shared parts (CC0 sources and the bodies, hair and motions built from them). See [parts/README.md](parts/README.md) (Japanese) |
| `distribution/` | Models and motions of the 4 originals (CC BY 4.0). File list and SHA-256 in `distribution/manifest.json` |
| `public/review-swing-2026-10-02/` | Food-themed weapons (CC BY 4.0) |
| `tools/character-maker/` | Asset builders, thumbnail capture, export checks, acceptance measurements |
| `docs/` | Screen design ([studio-quality-spec.md](docs/studio-quality-spec.md)), main-character features ([main-character-spec.md](docs/main-character-spec.md)), routes to yukikau.jp ([yukikau-route.md](docs/yukikau-route.md)) — in Japanese |

## Checks

```sh
pnpm typecheck
pnpm test
pnpm assets:check
node tools/character-maker/measure-studio.mjs /tmp/measure     # acceptance measurements per viewport (with pnpm dev running)
```

Browser E2E tests (`lib/character-maker/*.e2e.mjs`) run against a running dev server: `node --import ./tests/ts-resolve.mjs lib/character-maker/<name>.e2e.mjs <output dir>`. Playwright is found in the npx cache and drives Google Chrome (headless) (`tools/lib/paths.mjs`). The target is `CHARACTER_MAKER_URL` (default `http://127.0.0.1:4284`).

## Known issues

- At the peak of the swing, the sword tip can touch the edge of the 3D view for a few frames
- The cake sword looks like a flat board, Nami's collar is blurry, the 4 comic-style characters' face textures look muddy, and some characters barely move when idle (model-side issues)
- The originals' display models are large (Nami about 25 MB), so the first load takes time

## License

- Code: [MIT](LICENSE)
- Assets: CC BY 4.0 (AI Arranger's 4 original characters and the weapons) and CC0 1.0 (Quaternius and VRoid β sources and their derivatives). Per-path terms in [LICENSE-ASSETS.md](LICENSE-ASSETS.md), sources in [NOTICE.md](NOTICE.md)
- Attribution for CC BY assets: "Character Studio assets by AI Arranger (CC BY 4.0)"

## Credits

Quaternius (Universal Base Characters / Modular Character Outfits - Fantasy / Universal Animation Library, CC0), VRoid Project (VRoid β CC0 samples). Made by AI Arranger.
