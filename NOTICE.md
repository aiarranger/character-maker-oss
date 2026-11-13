# NOTICE

Character Studio (character-maker) — Copyright (c) 2026 AI Arranger. Code: MIT. Assets: see LICENSE-ASSETS.md.

## Third-party assets (CC0 1.0)

- **Quaternius** — Universal Base Characters, Modular Character Outfits - Fantasy, Universal Animation Library (Standard).
  https://quaternius.com/ — CC0 1.0. License texts: `parts/sources/*/License*.txt`. Provenance and SHA-256: `parts/sources/manifest.json`, `parts/sources/animations/provenance.json`.
- **VRoid Project / pixiv** — VRoid β CC0 sample models: Victoria Rubin, 桜田史利矢 (Sakurada Fumiriya), HairSample Male. CC0 1.0.
  Provenance and the official announcement URLs: `parts/sources/fantasy-v1/README.md`, `parts/sources/fantasy-v1/provenance.json`.
  These are the individual CC0 releases from the VRoid β period, not the current VRoid Studio AvatarSample A–Z (different terms).

Attribution is not required for CC0, but we credit the authors with thanks.

## Generative tools

The original characters (ハル, ナミ, ルクス, モカ) and the food weapons were made with ChatGPT image generation and Tripo, then edited by AI Arranger. See LICENSE-ASSETS.md §1.

## npm dependencies

Installed by the package manager, not bundled in this repository: Next.js, React (MIT); three.js, sharp, @gltf-transform/* and others for the asset build tools and tests only (various permissive licenses). See each package's LICENSE in `node_modules` after `pnpm install`.

## Copied third-party code

- **three.js r178** (MIT, Copyright © 2010-2025 three.js authors): the math classes in `lib/engine/r178/*.mjs`, and the OrbitControls, Camera and Object3D logic in `lib/engine/orbit-controls.ts`, `camera.ts`, `node.ts` (copied through the aisns project).
- **@types/three 0.178.1** (DefinitelyTyped, MIT, Copyright (c) Microsoft Corporation): the type declarations `lib/engine/r178/*.d.mts`. License text: `lib/engine/r178/LICENSE-types`.
- The Rust crates in `vendor/rust/` (island-kit and its character crates) are copied from the AI Arranger characters project (MIT).
