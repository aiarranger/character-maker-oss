# NOTICE

Character Studio (character-maker) — Copyright (c) 2026 AI Arranger. Code: MIT. Assets: see LICENSE-ASSETS.md.

## Third-party assets (CC0 1.0)

- **Quaternius** — Universal Base Characters, Modular Character Outfits - Fantasy, Universal Animation Library (Standard).
  https://quaternius.com/ — CC0 1.0. License texts: `parts/sources/*/License*.txt`. Provenance and SHA-256: `parts/sources/manifest.json`, `parts/sources/animations/provenance.json`.
- **VRoid Project / pixiv** — VRoid β CC0 sample models: Victoria Rubin, 桜田史利矢 (Sakurada Fumiriya), HairSample Male. CC0 1.0.
  Provenance and the official announcement URLs: `parts/sources/fantasy-v1/README.md`, `parts/sources/fantasy-v1/provenance.json`.
  These are the individual CC0 releases from the VRoid β period, not the current VRoid Studio AvatarSample A–Z (different terms).

- **Fish Kit** — fish models used as the enemies in `games/gyogun/` and as racers and spectators in `games/gyogyo/`. CC0 1.0. https://cda-social.github.io/fish/gallery.html — license text: `games/gyogun/public/models/fish/LICENSE.txt`, `games/gyogyo/public/models/fish/LICENSE.txt`.
- **Squid Kit** — squid shapes and photos (アオリイカ, コウイカ) used as racers in `games/gyogyo/`; the shape and motion code of `squid-kit.js` was ported to Rust (`games/gyogyo/rust/game/src/view/squid_shape.rs`, `squid.rs`). CC0 1.0. https://cda-social.github.io/fish/gallery#squid — license text: `games/gyogyo/public/squid/LICENSE.txt`.
- **KayKit Forest Nature Pack** (Kay Lousberg) and **Windmill** (Quaternius) — the sources of the trees, bushes and the windmill in `games/gyogyo/public/aisns/models/wilds/props/` (recoloured and reshaped by AI Arranger). CC0 1.0.
- **Kenney** (www.kenney.nl) — Impact Sounds, Interface Sounds, RPG Audio, converted to MP3 for `games/gyogun/public/aisns/sounds/wilds/` and `games/gyogyo/public/aisns/sounds/wilds/`. CC0 1.0. File-by-file sources: `CREDITS.txt` in that folder.

Attribution is not required for CC0, but we credit the authors with thanks.

## Generative tools

The original characters (ハル, ナミ, ルクス, モカ) and the food weapons were made with ChatGPT image generation and Tripo, then edited by AI Arranger. See LICENSE-ASSETS.md §1.
In `games/gyogun/`: きゃしー (Cassie) and the pickup models were made with Tripo and edited by AI Arranger; the eight sound effects in `public/sfx/` were generated with Stable Audio 3; Cassie's voice line was generated with Gemini TTS; the four music tracks in `public/music/` were generated with Stable Audio 3. See LICENSE-ASSETS.md §4.
In `games/gyogyo/`: the item and weapon models were made with Tripo and edited by AI Arranger; the music and the eight sound effects in `public/sfx/` were generated with Stable Audio 3. See LICENSE-ASSETS.md §5.

## Fonts (SIL OFL 1.1)

`games/gyogun/rust/game/assets/fonts/`: Zen Kaku Gothic New, Shippori Mincho B1, Dela Gothic One, Yuji Syuku (subset to the glyphs the game uses). License texts: `games/gyogun/rust/game/assets/fonts/licenses/`.
`games/gyogyo/rust/game/assets/fonts/`: Zen Maru Gothic, Dela Gothic One, Fredoka (subset). License texts: `games/gyogyo/rust/game/assets/fonts/licenses/`.

## npm dependencies

Installed by the package manager, not bundled in this repository: Next.js, React (MIT); three.js, sharp, @gltf-transform/* and others for the asset build tools and tests only (various permissive licenses). See each package's LICENSE in `node_modules` after `pnpm install`.

## Copied third-party code

- **three.js r178** (MIT, Copyright © 2010-2025 three.js authors): the math classes in `lib/engine/r178/*.mjs`, and the OrbitControls, Camera and Object3D logic in `lib/engine/orbit-controls.ts`, `camera.ts`, `node.ts` (copied through the aisns project).
- **@types/three 0.178.1** (DefinitelyTyped, MIT, Copyright (c) Microsoft Corporation): the type declarations `lib/engine/r178/*.d.mts`. License text: `lib/engine/r178/LICENSE-types`.
- The Rust crates in `vendor/rust/` (island-kit and its character crates) are copied from the AI Arranger characters project (MIT).
- `games/gyogun/` (game code and the Rust crates in `games/gyogun/vendor/rust/`) is copied from the beach stage of AI Arranger's game 百菓繚乱 and from the characters project (MIT).
- `games/gyogyo/` (game code and the Rust crates in `games/gyogyo/vendor/rust/`) is copied from AI Arranger's game かけっこグランプリ and from the characters project (MIT).
