# island-kit

three.js の代わりの小さな土台。浮島のゲーム（pengo・digdag・iceclimber・rainbow・bubble2・pinball・keiba とひな形）が使う。Chrome の WebGPU 専用。計画の正本は skills の `build-aiarranger-3d-game/references/rust-migration.md`。

three の完全な複製は作らない。7 本が実際に使う物だけを、three と同じ式で持つ（色・光・霧・影・並びの結果が同じになるように）。迷ったら 7 本の `src` を grep し、式は `<ゲーム>/node_modules/three/src` を読んで写す。

## 作り

| ファイル | 役 |
|---|---|
| `src/math.rs` | `Color`（16 進 → 線形）・オイラー角 |
| `src/geometry/` | 形（three と同じ頂点・法線・UV の式）・曲線・earcut・結合 |
| `src/scene.rs` | 節点の木（親子・位置・回転・大きさ・visible・renderOrder・layers・世界の行列） |
| `src/material.rs` | 材質の設定（Basic・Lambert・Soft・Phong・Standard・Sprite・Points・Custom） |
| `src/texture.rs` | CanvasTexture（OffscreenCanvas と 2D コンテキスト）・DataTexture・画像 |
| `src/glb.rs` | GLB の模型を場面へ読む（`character_actor::glb`） |
| `src/render/` | wgpu の描画（`mod.rs` Kit・`frame.rs` 1 コマの流れ（並び・まとめ描き・影・光）・`pipes.rs` パイプライン・`post.rs` 後処理） |
| `shaders/` | WGSL（GLSL と同じ式）。`env.wgsl` は環境の映り込みの PMREM を作る式（`src/render/env.rs`） |
| `src/app/` | 輪（固定の刻みと rAF）・入力・Web Audio・DOM・localStorage（wasm32 だけ） |

## 決めごと（three と同じになる所）

- 的: 色 `Rgba16Float`（既定は MSAA 4 倍。`kit.set_aa` で 1 サンプルにもできる。下の「縁のなめらかさ」）・深度 `Depth32Float`・自機の型 `R8Unorm`（1 サンプル）。`character_view::ActorTargets` もこの形式。
- 既定の深度比較は `LessEqual`（three の LessEqualDepth。`gl_Position = p.xyww` の空が写る）。
- 並び（three の painterSortStable）: 不透明は renderOrder → material id → z 昇順 → 作った順。半透明（`transparent: true`）は renderOrder → z 降順 → 作った順。
- 色: three の Lambert は `diffuse / π × (半球 + 環境 + Σ 方向の色 × dotNL × 影 + Σ 点の色 × 減衰 × dotNL)`（r155 以降の値。π を掛けない）。霧は FogExp2 `1 - exp(-d²z²)`。出力は線形の HDR（後処理の最後に sRGB）。
- 影: PCFSoftShadowMap の式（`shadowmap_pars_fragment.glsl`）をそのまま。`shadow.intensity`・`bias`・`normalBias` を持つ。影の型は FrontSide の材質なら裏面を描く（DoubleSide は両面、BackSide は表面）。
- CanvasTexture と画像は flipY: 上下を反転して上げる（three と同じ UV になる）。GLB と DataTexture は反転しない。sRGB の指定があれば `Rgba8UnormSrgb`、無ければ `Rgba8Unorm`。ミップ付き。
- ブレンド: Normal = (SrcAlpha, OneMinusSrcAlpha / One, OneMinusSrcAlpha)、Additive = (SrcAlpha, One / One, One)、Multiply の premultiplied = (Dst, OneMinusSrcAlpha / Zero, One)、Custom は係数をそのまま。polygonOffset は `slope_scale = factor, constant = units`。
- Points は wgpu に点の大きさが無いので、1 点ごとの板（インスタンス描画の 6 頂点）にする。`gl_PointCoord` の代わりを WGSL の補助で渡す（y は上が 0）。
- 性能の学び: 後処理の板に MSAA をかけない／影絵は別の render にしない（主の的と同じパスで描く）／骨は 1 コマ 1 回だけ送る／動的な属性は描く数だけ送る／同じ形と材質はまとめ描きする。

## 形（`src/geometry/`）の入口

`Geometry` は `positions`・`normals`・`uvs`・`colors`・`sway`・`indices`・`groups`（`mod.rs`）。`use island_kit::geometry::*;` で全部入る。three の `new THREE.XGeometry(...)` に当たる関数は同じ引数の順。短い名前はよく使う引数だけで、残りは three の既定。`_full` は全部の引数。

| three | ここ |
|---|---|
| PlaneGeometry | `plane(w, h)`・`plane_seg(w, h, ws, hs)` |
| BoxGeometry | `box_(w, h, d)`・`box_seg(w, h, d, ws, hs, ds)` |
| SphereGeometry | `sphere(r, ws, hs)`・`sphere_full(r, ws, hs, phiStart, phiLen, thetaStart, thetaLen)` |
| CylinderGeometry | `cylinder(rt, rb, h, rs)`・`cylinder_seg(.., rs, hs)`・`cylinder_full(.., rs, hs, open, thetaStart, thetaLen)` |
| ConeGeometry | `cone(r, h, rs)`・`cone_full(r, h, rs, hs, open, thetaStart, thetaLen)` |
| CircleGeometry・RingGeometry | `circle(r, seg)`・`circle_full(.., start, len)`・`ring(ir, or, ts)`・`ring_full(ir, or, ts, ps, start, len)` |
| TorusGeometry | `torus(r, tube, rs, ts)`・`torus_full(.., arc)` |
| LatheGeometry | `lathe(&[Vec2], seg)`・`lathe_full(.., phiStart, phiLen)` |
| CapsuleGeometry | `capsule(r, h, cs, rs)`・`capsule_full(.., hs)` |
| 多面体 | `icosahedron(r, detail)`・`octahedron`・`tetrahedron`・`dodecahedron`・`polyhedron(&verts, &idx, r, detail)` |
| ShapeGeometry | `shape_geometry(&shape, curveSegments)`・`shapes_geometry(&[shape], curveSegments)`（形ごとの group 付き） |
| ExtrudeGeometry | `extrude(&shape, &ExtrudeOpts { depth: 0.2, bevel_size: Some(0.1), ..Default::default() })`・`extrude_many(&[shape], &opts)` |
| TubeGeometry | `tube(&curve, tubular, r, radial)`・`tube_full(.., closed)` |
| RoundedBoxGeometry | `rounded_box(w, h, d, segments, radius)` |
| Shape・Path | `Shape::new()`・`Shape::from_points(&[DVec2])`・`Path::new()`。`move_to`・`line_to`・`quadratic_curve_to`・`bezier_curve_to`・`spline_thru`・`arc`・`absarc`・`ellipse`・`absellipse`・`close_path`・`get_points(div)`。穴は `shape.holes.push(path)` |
| CatmullRomCurve3 | `CatmullRomCurve3::new(&[Vec3])`・`::with(&pts, closed, CurveType::CatmullRom, tension)`。`get_point`・`get_point_at`・`get_tangent`・`get_tangent_at`・`get_length`・`get_points`・`get_spaced_points`・`compute_frenet_frames` |
| mergeGeometries | `merge_geometries(&[&a, &b], use_groups) -> Option<Geometry>` |

メソッドは `rotate_x`・`rotate_y`・`rotate_z`・`translate`・`scale`・`center`・`apply_matrix4(Mat4)`・`compute_vertex_normals`・`normalize_normals`・`to_non_indexed`・`bounding_box`・`bounding_sphere`。頂点色と揺れの量も一緒に動く。変形は `&mut self` を返すので、TS の `new THREE.PlaneGeometry(1, 1).rotateX(a)` は `plane(1.0, 1.0).rotate_x(a).owned()` と書く。

three と一致する試験は `tests/geometry.rs`。比べる値は `tests/fixtures/gen-geometry.mjs` が three で作る。

GPU を使う試験は `tests/gpu.rs`（Mac の Metal で `Kit::headless` を描いて読み戻す。ミップの一致・上げ直し・形の破棄・storage・別のカメラ・透過用の描画・シェーダーのエラー・モーフ・Device の使い回し）。普通の `cargo test` でも回るが、GPU の鍵を取って流す: `python3 ../scripts/gpulock.py run 'island-kit gpu' -- cargo test -p island-kit --test gpu`。

## 輪・入力・音・DOM・保存（`src/app/`）

- `raf`: `start(f)`・`drive(ms, f)`・`now_ms()`・`after_ms(ms, f)`・`FrameClock`（`tick(now)` が 1 コマの秒数）・`FixedStep`（`add(dt)`・`while step.next() { .. }`・`clear()`）
- `input`: `Input::new(InputOpts { touch_root: Some(dom::el("touch")), stick_four_way: true, ..Default::default() })`。毎コマ `read()` を 1 回呼び、`Frame` の `down(code)`・`pressed(code)`・`mouse_pressed(0)`・`pad`（最初の 1 つ）・`pads`（全部。4 つまで。同じ画面の協力用）・`touch`・`btn_pressed("push")`・`device` からゲームの Controls を作る。押した瞬間を固定の刻みへ渡すときは `Latch`。
  機器（`device`・`touch.active`・body の class `touch`）の決め方は今の版の `input.ts` がゲームごとに違うので、次の 3 つを `InputOpts` で選ぶ（表は `src/device.rs`）。
  | ゲーム | 指定 | 決め方 |
  |---|---|---|
  | pengo・digdag・iceclimber・rainbow・bubble2 | `touch_class_follows_device: false`（既定） | 機器を追わない（TS の `input.touch`）。最初の touchstart か `(pointer: coarse)` で `touch.active` と class が付き、外れない。`device` はこの印の写しで、キー・マウス・パッドでは変わらない |
  | keiba・ひな形 | `touch_class_follows_device: true` | keydown と canvas の mousedown でキー、`touch_root` の touchstart でタッチ、最初のパッドでパッド。class は機器に合わせて付け外し。window の touchstart は見ない。指を離した後のまねの mousedown が canvas に来るとキーへ戻るのも今の版と同じ（遊びの中は `touch_root` が canvas を覆うので、canvas には来ない） |
  | pinball | `touch_class_follows_device: true` と `pointer_ignore: Some(..)` | keydown でキー、`pointer_ignore` に当たらない pointerdown の pointerType でタッチかキー、最初のパッドでパッド。mousedown と touchstart は見ない |
- `audio`: `AudioKit::new(AudioOpts::default())`（既定は pengo の値）。`init`・`osc`・`noise`・`sample`・`sample_limited`・`bell`・`load_sample`・`music`・`music_stop`・`jingle`・`set_muted`・`duck`。風や歓声のようなゲーム固有の音は `ctx()`・`sfx_bus()`・`reverb()` から作る
- `media`: 曲とジングルの `<audio>` を使い回す `MediaSlots`（`audio` が使う。自前で曲を鳴らすゲームも `start` で使う）。iPhone・iPad の WebKit（iPad の Chrome も同じ）は、操作の直後に一度 play した `<audio>` でないと後から play させない。そのため最初の操作で全部を鳴らせるようにしておき、操作の前に頼まれた曲（オープニング）はその時に鳴らす。`HtmlAudioElement` を曲ごとに新しく作らない
- `dom`: `el(id)`・`set_text`・`set_html`・`class_toggle`・`set_style`・`on(id, "click", f)`・`create(tag, class)`・`query_all`・`closest`・`listen`
- `store`: `get`・`set`・`get_json`・`set_json`（失敗は無視）

## 使い方

残りのゲームの担当はここから始める。動く例は pengo の `rust/game/src/smoke.rs`（全機能を一度通す画面。`?smoke` で開く）と `rust/game/src/view/`（ゲームの見た目を全部移した物）、同じ場面を three で組んだのが `rust/compare/min-three.ts`、撮って比べる道具が `rust/compare/shot.py`・`diff.py`。

### 始め方

```bash
node <characters>/scripts/game-parts.mjs --to <ゲーム> --add-rust --write   # vendor/rust へ写す。crates を変えたら --write で回す
```

`rust/game/Cargo.toml` は pengo と同じ形（`crate-type = ["cdylib", "rlib"]`、`island-kit`・`character-actor`・`character-view` は `../../vendor/rust/` の path、`wgpu = "30"`、`glam`、`bytemuck`、wasm32 に `wasm-bindgen`・`wasm-bindgen-futures`・`js-sys`・`console_error_panic_hook`・`web-sys`）。`rust/web/build.sh` は pengo のものを写して名前を変える（今の `index.html` と `src/style.css` と `public/` を dist へ置き、script の入口だけ wasm へ差し替える）。配信は台帳の固定ポート。

```rust
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async { run().await.unwrap() });
}
async fn run() -> Result<(), String> {
    let canvas: web_sys::HtmlCanvasElement = dom::el("gl").dyn_into().unwrap();
    let mut kit = Kit::new(canvas).await?;        // WebGPU が無ければ Err
    kit.camera = Camera { fov: 34.0, near: 0.3, far: 900.0, ..kit.camera };  // aspect は kit が窓に合わせる
    let geo = kit.geometry(&sphere(0.8, 32, 20));
    let mat = kit.material(Material::soft().with_color(Color::css("#e2506e")));
    let n = kit.scene.mesh(None, geo, mat);       // None は場面の根へ
    kit.scene[n].position = Vec3::new(0.0, 1.0, 0.0);
    kit.scene[n].cast_shadow = true;
    // Rc<RefCell<..>> に入れて raf::start で回す。毎コマ kit.render(&RenderOpts { outline, hidden, clear })
    Ok(())
}
```

### three の何 → kit の何

| three | kit |
|---|---|
| `scene`・`camera`・`scene.fog` | `kit.scene`・`kit.camera`・`kit.fog = Some(FogExp2 { color, density })` |
| `new THREE.Group()` / `new THREE.Object3D()` | `scene.group(parent)` / `scene.object(parent)`（Group の renderOrder は子の並びの先頭の鍵。three と同じ） |
| `new THREE.Mesh(geo, mat)`・配列の材質 | `scene.mesh(parent, geo, mat)`・`scene.mesh_multi(parent, geo, &[mats])`（geometry.groups の material 番号で引く） |
| `new THREE.InstancedMesh(geo, mat, n)` | `scene.instanced(parent, geo, mat, n)`・`set_instance`・`set_instance_color`・`set_instance_count`・`write_instance_attr` |
| `new THREE.BatchedMesh(n, v, i, mat)` | `scene.batched(parent, mat)`（下の「BatchedMesh」） |
| `SkinnedMesh`・`Skeleton`・`SkeletonUtils.clone(gltf.scene)` | `glb::instantiate_skinned(&mut kit, parent, &glb, &mut \|kit, m\| mat)` が 1 体を置いて `SkinnedModel { root, nodes, meshes, skeletons }` を返す（下の「骨付き」）。手で組むなら `kit.geometry_skinned(&geo, joints, weights)`・`scene.new_skeleton(bones, inverses)`・`scene.skinned(parent, geo, mat, skeleton)`。`mesh.boundingSphere = ..` は `scene.set_skinned_bounds(id, c, r)`、`skeleton.dispose()` は `scene.remove_skeleton(id)`（節点は別に `remove`） |
| `new THREE.Points`・`LineSegments`・`Sprite` | `scene.points(parent, cap, mat)`・`scene.lines(parent, cap, mat)`・`scene.sprite(parent, mat)`（`set_sprite_center`） |
| 光 | `scene.dir_light(parent, color, intensity)`・`hemi_light`・`ambient_light`・`point_light`・`spot_light`。値は `scene.dir_light_mut(id)` など。方向の光の向く先は `.target = Target::Pos(p)` か `Target::Node(id)`。影は `scene[id].cast_shadow = true` と `.shadow` |
| `o.position`・`o.scale`・`o.visible`・`o.renderOrder`・`o.castShadow`・`o.receiveShadow`・`o.frustumCulled`・`o.name` | `scene[id].position` など同じ名前の pub フィールド（`render_order`・`cast_shadow`…）。`set_scale(s)` は setScalar |
| `o.rotation.y = a`・`o.rotation.set(x, y, z)`・`o.quaternion.copy(q)` | `scene[id].rotation.set_y(a)`・`= Rot::euler(x, y, z)`（順は `Rot::euler_order`）・`= Rot::Quat(q)` |
| `o.layers.enable(1)`・`disable(0)` | `scene[id].layers |= 2`・`&= !1` |
| `o.matrixAutoUpdate = false; o.matrix` | `scene[id].matrix = Some(m)` |
| `o.lookAt(p)`・`getWorldPosition`・`matrixWorld`・`traverse`・`getObjectByName`・`clone(true)`・`remove`・`clear` | `scene.look_at(id, p)`・`world_position`・`world_matrix`・`traverse`・`get_by_name`・`clone_subtree`・`remove`（子ごと）・`clear_children`・`set_parent` |
| `new Box3().setFromObject(o)` | `kit.box_of(id)` |
| `camera.lookAt`・`vector.project(camera)` | `kit.camera.look_at(p)`・`kit.camera.project(p)`（NDC。z も three の -1..1） |
| `MeshBasicMaterial` | `Material::basic()` |
| `MeshLambertMaterial`・`MeshPhongMaterial`・`MeshStandardMaterial` | `Material::lambert()`・`phong()`・`standard()`。値は `kind: MatKind::Phong(Phong { emissive, emissive_intensity, specular, shininess, flat_shading })` など |
| `emissiveMap` | 材質の `emissive_map`（Lambert・Soft・Phong・Standard。tex1 を使う。Custom と Physical は別の物が tex1） |
| `MeshPhysicalMaterial`・`scene.environment`・`PMREMGenerator.fromScene(new RoomEnvironment(), sigma)`・`scene.environmentIntensity` | `Material::physical(Physical { emissive, emissive_intensity, metalness, roughness, clearcoat, clearcoat_roughness, sheen, sheen_color, sheen_roughness, env, env_intensity, flat_shading })`。`env` は `kit.room_environment(sigma)` が返す的（起動時に 1 回作る。部屋の絵は RoomEnvironment だけ）、`env_intensity` が environmentIntensity。式は `kit.wgsl` の `MODE_PHYSICAL` と `env.wgsl`（ior 1.5・f90 1 だけ。transmission・iridescence・anisotropy は持たない）。Standard には環境を効かせない（three は効く。使うゲームが無い） |
| pengo の `softMaterial` | `Material::soft()`・`MatKind::Soft(Soft { emissive, emissive_intensity, rim, sway, flat_shading })` |
| `SpriteMaterial`・`PointsMaterial`・`LineBasicMaterial` | `Material::sprite()`・`points(size)`・`line_basic()`（`MatKind::Sprite { rotation, size_attenuation }` など） |
| `ShaderMaterial` | `Material::custom(CustomShader { .. })`（下） |
| `transparent`・`opacity`・`depthWrite`・`depthTest`・`depthFunc`・`side`・`blending`・`premultipliedAlpha`・`polygonOffsetFactor/Units`・`fog`・`vertexColors`・`alphaTest`・`visible`・`colorWrite` | 同じ意味の pub フィールド（`depth_func: wgpu::CompareFunction`、`blending: Blending::Additive`・`Custom { src, dst }`、`polygon_offset: Some((factor, units))`） |
| `material.color.set(..)`・`opacity = ..`（毎コマ） | `kit.mat_mut(id).opacity = ..`。`material.clone()` は `kit.clone_material(id)` |
| `texture.repeat`・`offset`・`rotation`・`center` | 材質の `uv: UvTransform`（three ではテクスチャの値。同じテクスチャを別の変換で使うなら材質を分けるだけ） |
| `CanvasTexture` | `kit.canvas_texture(w, h, srgb, \|ctx\| ..)`（`ctx` は OffscreenCanvasRenderingContext2d）。`needsUpdate` は `kit.redraw_canvas(id, \|ctx\| ..)`。文字に字体を使う前に `texture::load_font("700 40px \"Zen Maru Gothic\"").await` |
| `wrapS`・`magFilter`・`anisotropy` | `kit.set_tex_opts(id, TexOpts { anisotropy: 8, ..TexOpts::repeat() })`（既定は three と同じ Clamp・線形ミップ） |
| `DataTexture`・`TextureLoader` | `kit.data_texture(w, h, &rgba, srgb)`（反転しない）・`texture::load_image(url).await` と `kit.image_data_texture(&img, srgb)` |
| WebGPUで混雑中の初回画像転送 | `kit.set_staged_texture_uploads(true)`（WASMのみ・既定false）。同じCPUミップを256-byte行へ詰め、1つのCOPY_SRC bufferからまとめて送る。上限を超える画像は従来経路。既存canvasの更新と画像cacheは変更しない。bubble2は起動時を従来経路に保ち、stage 1以降だけ有効にする。全画像への適用は上限解除時の初期停止を悪化させたため採用していない |
| `WebGLRenderTarget` と `setRenderTarget` | `kit.render_target(w, h)`・`kit.render_to(tex, &camera)`（後処理なし・影の型はそのまま）。材質の `map` に貼れる。`readRenderTargetPixels` は `kit.read_target(tex).await`（上の行が先） |
| `Reflector`（鏡・水面の映り込み。行列を直接渡したカメラで的へ描く） | `kit.render_view_to(tex, &RawView, &ViewDraw { hide, clear, clear_alpha })`（下の「別のカメラで的へ描く」） |
| `MeshPhysicalMaterial` の transmission の別描画（renderTransmissionPass） | `kit.transmission = Some(Transmission { scale, exclude })`・`kit.transmission_target()`（下の「透過用の別描画」） |
| `texture.image` の書き換えと `needsUpdate`（DataTexture の全体・一部） | `kit.write_texture(tex, x, y, w, h, &rgba)`（新しい GPU の画像を作らない。下の段は「画像の上げ直し」） |
| `geometry.dispose()` | `kit.destroy_geometry(geo)`（GPU の頂点と添字を今すぐ destroy。その形の節点は描かれない。`update_geometry` で戻る） |
| `generateMipmaps`（GPU で作る） | `kit.set_gpu_mips(true)`（既定 false は CPU。このあとに作る画像と上げ直しが GPU で作る。出る値は CPU と同じ） |
| WebGL のコンテキストを場所をまたいで使い回す | `Gpu::new().await`・`Kit::with_gpu(canvas, &gpu)`・Kit を手放す前に `kit.release_gpu()`（下の「Device を使い回す」） |
| `GLTFLoader`（`EXT_mesh_gpu_instancing` は InstancedMesh になる） | `glb::load_glb(url).await`・`glb::instantiate(&mut kit, parent, &glb, &mut \|kit, m\| mat)`・`glb::instantiate_node`（v0 などを選ぶ）・`glb::merged_geometry(&glb, &\|node\| true)`・`kit.glb_texture(&glb, image)`・`glb::standard_material`（GLTFLoader と同じ値の材質） |
| GLTFLoader の `COLOR_0` → `vertexColors` | `glb::standard_material` の材質（その写しも）は、`instantiate`・`instantiate_node` が COLOR_0 のある部分だけ頂点色を入れる。頂点色の無い部分とゲームが自分で作った材質はそのまま。部分を自分で置くゲームは `glb::primitive_material(kit, mat, primitive)` を通す。ゲーム側で `vertex_colors = true` を入れる対処（iceclimber の `view/backdrop.rs` の小屋）は要らなくなった（残っていても同じ絵）。主役（character-view）は前から頂点色を掛けている |
| `SHARED.uTime`・`uRimColor`・`uRimI`・`uDim`・`uLamps`・`uCloudK` | `kit.shared.time`・`rim_color`・`rim_i`・`dim`・`lamps`・`cloud_k`（digdag の流れる雲の影。Soft だけ。既定 0 は何も足さない） |
| `scene.fog = new Fog(c, near, far)` | `kit.fog_linear = Some(FogLinear { color, near, far })`（`fog` より優先） |
| `camera.setViewOffset(W, H, x, y, W, H)` | `kit.camera.view_offset = (x / W, y / H)`（同じ大きさの窓をずらす形だけ）。`project` も読む |
| R3F の `<Canvas>`（EffectComposer なしで画面へ直接描く） | `kit.output = Output::DirectFlat`（`flat`）・`Output::DirectAces`（既定の ACES）と `kit.set_final(FINAL_DIRECT, 16)`・`kit.bloom_enabled = false`。材質ごとに色調 → sRGB → 霧 → premultiplied の順で出し、ブレンドと MSAA が three と同じ sRGB の値で行われる。`toneMapped` は材質の `tone_mapped`。Custom は `shade_out(rgb, a, depth, fog, tone, premult)` で返す（`colorspace_fragment` を書かない ShaderMaterial は `vec4f(rgb, a)` のまま） |
| three r182 以降の光のエネルギー（直接光のつやの多重散乱の補正・つやの分を拡散から引く） | `kit.three_r186 = true`（既定 false は r178 の式。Engimeow の three は 0.186）。DFG は近似式のまま |
| `MeshBasicMaterial` に夕日の陰と縁の光（Engimeow の RimLit の basic の分岐） | 材質の `view_rim`（Basic でも効く。指数 3、Standard は 2.4） |
| `MeshStandardMaterial` の `bumpMap`・`bumpScale` | 材質の `bump_map`・`bump_scale`（Standard だけ。`map` と同じ UV。tex1 を使うので `emissive_map` とは同時に使えない。`data_texture(.., false)` で線形に。式は `bump_normal`） |
| r182 以降の `PCFShadowMap`（R3F の `shadows`。`PCFSoftShadowMap` は消えて、これになる）・`shadow.radius` | `kit.shadow_vogel = true`（5 点の Vogel の円板を IGN で回す。1 点ごとに 4 つの比較を混ぜる＝ three の sampler2DShadow の LinearFilter。半径は `shadow.radius`。既定 false は r180 までの式で、pengo などは変わらない） |
| `renderer.shadowMap.autoUpdate`・`needsUpdate` | `kit.shadow_auto_update`（既定 true で毎コマ描く）・`kit.shadow_needs_update`（描くと false に戻る）。切っている間は影の型も光の行列（three の `shadow.matrix`）も前に描いたときのまま。`cast_shadow` を入れても、頼むまで影は出ない。切っている間に、three では幕の前に読み終わっている物を非同期で足したら、ゲームが `shadow_needs_update = true` を頼む（頼まないと次の描き直しまで影が無い）。まだ描いていない・的の大きさが変わったときは頼みがなくても描く（中身が無いので）。回数と行列は `kit.shadow_read()` |
| drei の `Hud`（`gl.clearDepth` のあと composer を通さず画面へ描く層） | `kit.overlay.push(OverlayQuad { tex, m, opacity })`。毎コマ入れ直す。m は -0.5..0.5 の板を clip へ写す行列。仕上げのあとに sRGB のままブレンドする。テクスチャ付きの板だけ |
| ページの一部の canvas・`alpha: true` | `kit.set_size_from_canvas(true)`（窓でなく canvas の表示の大きさ）・`kit.set_canvas_alpha(true)` と `kit.clear_alpha = 0.0` |
| `view.bloom.strength` | `kit.bloom.strength`（`radius`・`threshold`）。`view.bloom.enabled` は `kit.bloom_enabled` |
| `bloom.setSize(w / 2, h / 2)`・`pixelCap` の画素数 | `kit.set_bloom_div(2)`・`kit.set_pixel_budget(2.0e6)`（pinball。既定は 1 と 2.6e6） |
| `view.final.uXxx` | ゲームの `FinalU` を `kit.set_final_uniform(bytes)` で毎コマ渡す（下） |
| `view.render(t, outline, hidden)` | `kit.shared.time = t; kit.render(&RenderOpts { outline, hidden, clear })` |
| `view.size`・`renderer.getPixelRatio()`・`view.resize()` | `kit.size()`・`kit.dpr()`・`kit.resize()`（窓が変わると render が自分で呼ぶ）。`view.setMaxPixel(v)` は `kit.set_max_pixel(v)`。`uResolution` は `kit.render_size()`、`uMaskPx` は `kit.mask_px()` |
| `view.adapt(ms)` | `kit.adapt(ms)` は pengo・digdag・iceclimber・rainbow・bubble2・keiba の `renderer.ts` と同じ式（直近の 1 コマが 24ms 超で slow を、13ms 未満で fast を数える。slow が 90 を超えると解像度を 0.25 下げ、0.75 で止める。fast が 600 を超えると 0.25 上げる）。**pinball の `renderer.ts` は別の式**（45ms 以上は数えない。平均 `avg += (ms - avg) * 0.05` が 18ms 超で slow を、13ms 未満で fast を数える。slow が 60 になると、最初の 1 回は MSAA を切り、次から解像度を 0.25 下げる）。pinball は `kit.adapt` を使わず、`rust/game/src/app.rs` の `adapt` が同じ式で `kit.set_max_pixel` を呼ぶ。kit の MSAA は遊びの途中では切れない（`kit.set_aa` は Kit を作ってすぐ、主役を入れる前だけ）ので、その段は数えるだけ |

### Custom のシェーダー（three の ShaderMaterial）

`CustomShader { label, wgsl, vs, fs, uniform, textures, instance_attrs, point_attrs }`。`wgsl` の先頭に kit が `character_view::COMMON` と `KIT_COMMON`（`shaders/kit_common.wgsl`）を付けるので、次がそのまま使える。

- `g`: Globals（`g.view`・`g.proj`・`g.view_proj`・`g.cam`・`g.fog`・`g.misc`（x = 時間、y・z = 的の高さと幅（px）、w = 画素の倍率））。three の `uTime` は `k_time()`、`cameraPosition` は `k_cam()`。
- `VIn`（`@location(0..4)`: pos・normal・uv・color・sway）と `IIn`（`@location(8..12)`）。`inst_model(i)` が **世界 × インスタンス**（three の `modelMatrix * instanceMatrix`。普通の Mesh も同じ入口）。`i.fx`: rgb = instanceColor（無ければ白）、w = receiveShadow。
- `apply_fog(col, depth)`（`depth` は `-mv.z`）・`normal_world(m, n)`・`lights`（uniform）・`dir_shadow(k, world, n)`・`distance_attenuation`。
- `@group(2) @binding(0)` がゲームの uniform（自分で `struct` を書く。256 バイトまで。vec4 にそろえると楽）。テクスチャは `tex0`/`smp0` … `tex5`/`smp5`（`textures` に渡した順。aisns の地面が 6 枚読む）。
- 256 バイトより大きい値は `kit.set_storage(mat, bytes)` で渡し、WGSL では `k_storage[i]`（`@group(2) @binding(13) var<storage, read> k_storage: array<vec4f>`。kit が前に付けるので宣言は書かない）。材質ごとに 1 つ。同じ大きさまでなら書き直すだけなので毎コマ呼んでよい。渡していない材質は 16 バイトの 0。
- シェーダーの作成エラーは材質ごとに閉じ込める: 作れない材質は描かずに飛ばし（ほかの物は描く）、`kit.take_shader_errors()` が「材質の label とエラーの文」を 1 回だけ返す（ブラウザのコンソールにも出る）。`kit.material_failed(mat)` で確かめられる。ブラウザでは確かめの結果があとのコマで届くので、届くまでの 1〜数コマは今までどおり画面が出ないことがある。新しい材質を確かめ終わるまで描かない `kit.set_wait_for_shader_check(true)` なら 1 コマも止めない（その分、新しい材質が出るのが 1〜数コマ遅れる。既定 false）。

```wgsl
struct U { p: vec4f };
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) depth: f32 };
@vertex fn vs(v: VIn, i: IIn, @location(13) a_jolt: vec4f) -> O {
  let mv = g.view * inst_model(i) * vec4f(v.pos, 1.0);
  var o: O; o.clip = g.proj * mv; o.uv = v.uv; o.depth = -mv.z; return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f { return vec4f(apply_fog(u.p.rgb, i.depth), 1.0); }
```

- 値を毎コマ変えるのは `kit.set_uniform(mat, bytemuck::bytes_of(&u))`。three の uniform の共有は無いので、同じ値を読む材質ごとに渡す。
- インスタンスの属性（InstancedBufferAttribute）は `instance_attrs` に形式を書く。**`@location` は 13, 14, 15, 5, 6, 7 の順**（WebGPU は 16 未満で、6 つまで）。値は `scene.write_instance_attr(id, 番号, &[f32])`。描く数だけ GPU へ送る。
- 普通の Mesh の独自の頂点属性（`smoothN`・`aU` など）は `Geometry.colors`（vec4 をそのまま渡す。`VIn.color`）か `sway`（float）に入れて読む。
- Points は頂点 6 つの板。`@location(0)` が位置、`IIn` は全部の点で同じ世界の行列、`point_attrs` は上と同じ順の `@location`。頂点の入口に `@builtin(vertex_index) vid: u32` を足し、`o.clip = point_clip(g.proj * mv, size_px, vid)`（`gl_PointSize` が `size_px`）、`point_uv(vid)` が `gl_PointCoord`（y は上が 0）。
- 空のように `gl_Position = p.xyww` と書くのは three と同じ（既定の深度比較が LessEqual）。
- 影の型（Custom も）は頂点を変えない位置で描く（three の depth 材質と同じ）。

### BatchedMesh（インスタンスごとの形の切り替えと間引き）

three の BatchedMesh の代わり（aisns の `WildsProps`・`WildsLandmarks`・`DistantWorlds`）。形の表とインスタンスの表を 1 つの節点に持ち、**毎コマ・視野ごと（主のカメラ・影の光）に**、描く印のあるインスタンスを形の包む球で間引いて形ごとに集め、1 コマのインスタンスの流れへ入れて描く（`src/render/batch.rs`）。

```rust
let b = kit.scene.batched(Some(group), mat);       // new THREE.BatchedMesh。材質は 1 つ
let near = kit.scene.batch_add_shape(b, near_geo); // addGeometry（kit.geometry で作った形。形の番号を返す）
let far = kit.scene.batch_add_shape(b, far_geo);
let i = kit.scene.batch_add_instance(b, near);     // addInstance（単位行列・白・描く印あり）
kit.scene.set_batch_matrix(b, i, m);               // setMatrixAt
kit.scene.set_batch_color(b, i, [a, b, c, d]);     // setColorAt（4 つの値）
kit.scene.set_batch_visible(b, i, false);          // setVisibleAt
kit.scene.set_batch_shape(b, i, far);              // setGeometryIdAt（遠近の切り替え）
kit.scene[b].cast_shadow = true;                   // 影の型も光の視錐台でインスタンスごとに間引く
kit.scene[b].receive_shadow = true;
```

- 遠近の距離の決め方はゲームが持つ（three でも BatchedMesh の外。aisns は `rust/game/src/parts/props.rs`）。kit は「今どの形か」を読むだけ。
- 材質は組み込みでも Custom でもよい。Custom は `i.fx` で 4 つの値をそのまま読む（`fx.w` は色の 4 つ目で、receiveShadow ではない）。組み込みは rgb を頂点色に掛け、`fx.w` は receiveShadow。`instance_attrs` のある Custom は使えない（列を流れの中で動かすので、別のバッファに同じ順で置けない）。
- 節点の `frustum_culled` は見ない。間引きを切るのは `Batch.per_object_culled = false`（three の perObjectFrustumCulled。既定 true）。
- 描く順は形の番号の順。three の `sortObjects`（手前から）は持たない（不透明では絵が変わらない）。先に描く方が塗り直しが減るので、**近景の形を先に足す**。半透明の材質で個体の前後が要る物には使えない。
- 層 1（縁取りと影絵）には載らない（使う物が無い）。
- 形ごとの InstancedMesh を毎コマ入れ直す形を取らない理由: 1 本の行列の列では主と影で別の間引きができず、影の範囲（±28〜40 m）の外の木まで影の型に描いてしまう。

### Points・線

```rust
let p = kit.scene.points(None, 64, mat);
kit.scene.set_point_positions(p, &xyz);     // x, y, z の列
kit.scene.write_point_attr(p, 0, &colors);  // point_attrs の 0 番目
kit.scene.set_point_count(p, n);            // geo.setDrawRange(0, n)
let l = kit.scene.lines(None, 16, mat);
kit.scene.set_line_positions(l, &xyz);      // 2 つで 1 本。包む球（視錐台の間引き）も作り直す
```

### 後処理

renderer.ts と同じ（保護 → UnrealBloomPass → 仕上げ）。仕上げはゲームごとに違うので `kit.set_final(wgsl, uniform_size)` で渡す（pengo は `rust/game/shaders/final.wgsl`）。前置きの `shaders/final_common.wgsl` が付くので、`VOut`（`i.uv` は three と同じ下が 0）・`smp(uv)`（tDiffuse。vec4）・`mask_at(uv)`（tMask）・`hash2`・`to_srgb`・`tex_uv` が読める。ゲームは次だけ書く。

```wgsl
struct FinalU { .. };
@group(1) @binding(0) var<uniform> u: FinalU;
@fragment fn fs_final(i: VOut) -> @location(0) vec4f { .. return vec4f(to_srgb(max(col, vec3f(0.0))), 1.0); }
```

renderer.ts の GLSL は式の順に写す。`mat2(a, b, c, d)` は `mat2x2f(a, b, c, d)` で同じ並び。`texture2D(tDiffuse, uv)` が `smp(uv)`。

### 縁のなめらかさ（`kit.set_aa`）

`kit.set_aa(Aa::Msaa4 | Aa::Off | Aa::Smaa)`。既定は `Msaa4`（今までと同じ。主の的・別のカメラの的・透過用の的・主役と NPC のパイプラインが 4 サンプル）。`Off` と `Smaa` は全部 1 サンプルで描き、的へ直接描く（解決のパスが無い）。`Smaa` は仕上げ（`fs_final`）の画を画面と同じ形式の的へ描いてから、SMAA の 3 つのパス（縁を見る → 混ぜる重み → 混ぜて画面へ）をかける。式は postprocessing 6.39.2 の `SMAAEffect`（Zlib）を `shaders/smaa.wgsl` へ写した物で、設定は HIGH・色で縁を見る・予測なし（aisns の three の版の `lib/color-smaa-effect.ts` と同じ）。three の版は線形の半精度の的にかけるので、仕上げの画は sRGB の見方で読み（線形で縁を見て混ぜる）、出すときに sRGB へ戻す。表（`shaders/smaa_area.rg8`・`smaa_search.r8`）は postprocessing の画像を 8 ビットの値にした物（wasm が約 175 KB 増える。gzip で約 40 KB）。

- Kit を作ってすぐ、主役（`set_actor`・`set_npc_actor`・`put_actor`）を入れる前に呼ぶ。主役のパイプラインは入れたときのサンプル数のままなので、`take_actor`・`put_actor` で持ち回るのは同じ設定の Kit の間だけ。もう作った別のカメラの的・透過用の的は作り直し、材質のパイプラインは次に描くときに作り直す。
- 仕上げのあとに重ねる板（`kit.overlay`）は SMAA の後に描く（縁を混ぜない）。

### 主役

```rust
let holder = kit.scene.group(None);
kit.scene[holder].set_scale(PLAYER_SCALE);
let actor = kit.scene.actor(Some(holder));           // 影は既定で落とす
kit.scene[actor].layers = 1 | 2;                     // 層 1 は縁取りと影絵に出る。点滅は &= !1（型と影絵は残る）
kit.set_actor(actor, Catalog::open_default_web().await?.hero_web("aichan", GAIT).await?);  // 読む間は kit を借りない
kit.set_actor_look(actor, ActorLook { rim, rim_strength, aura, aura_strength, top, lift, flash, hidden, hidden_opacity });
kit.set_actor_style(ActorStyle::PENGO);              // 輪郭の光の式。既定は PENGO。keiba・ひな形は MUSO、pinball は PINBALL
kit.set_actor_fade(actor, 0.4);                      // カメラと自分の間の走者を網目に抜く（keiba の uFade。既定 0）
kit.set_actor_sun_rim(actor, color, 1.35);           // 夕日の陰と縁の光（Engimeow の RimLit。既定 0 で何もしない）。光ありの材質には `view_rim: Some((color, k))`
kit.set_actor_weapon(actor, &weapon_glb);            // 武器の模型。持たせるのは actor.hold_weapon
kit.set_actor_weapon_unlit(actor, true);             // 武器を光なしで描く（three の MeshBasicMaterial。aisns。既定は光あり）
// 毎コマ（順番は crates/README.md。ゲームの動きの更新 → 入れ物の位置 → これ → kit.render）
let world = kit.actor_world(actor);
let a = kit.actor_mut(actor);
a.set_parent_world(world);
a.move_(speed, grounded, armed, None);
a.express(face);                                     // 表情（Option<FaceControl>。台帳に表情のある主役だけ効く。どの場面で出すかはゲーム）
let events = a.update(dt, now);
```

`kit.render` が `write_frame` を 1 回呼ぶ。同じ人をもう 1 体は `kit.share_actor(from, to, 読み直した actor)`（GPU の形を共有）。半透明の体と ＞＜ の板は renderOrder 0・入れ物の z で半透明の列に混ざる。

### 骨付き（SkinnedMesh）

主役でない骨付きの物（aisns の地域の敵・ボスなど）。形とテクスチャは glb ごとに 1 回だけ GPU へ上げて共有し、節点と骨と動きは 1 体ごとに持つ。骨は場面の節点なので、動かすのは節点の TRS。

```rust
let g = glb::load_glb(url).await?;                    // 読むのは kit を借りずに
let model = glb::instantiate_skinned(&mut kit, Some(holder), &g, &mut |kit, m| glb::standard_material(kit, &g, m));  // 2 体目も同じ呼び方で
let mut skel = character_actor::anim::Skeleton::from_glb(&g);   // 1 体に 1 つ（Clone してよい）
let clips = character_actor::anim::Clip::all_from_glb(&g, &skel);
let mut mixer = character_actor::anim::Mixer::new();
let a = mixer.add(clips[0].clone());                  // mixer.clipAction
mixer.play(a, &skel);
// 毎コマ: 自分の時計なら mixer.advance(dt, &mut skel)。TS の action.time をそのまま使うなら action の time を置いて advance(0.0, ..)
mixer.actions[a].time = t;
mixer.advance(0.0, &mut skel);
// 動きの値（番号は GLB の節点の番号）を節点へ写す
for (i, n) in model.nodes.iter().enumerate() { if let Some(n) = *n { let o = &mut kit.scene[n]; o.position = skel.t[i]; o.rotation = Rot::Quat(skel.r[i]); o.scale = skel.s[i]; } }
```

- three と同じ: 世界の位置は「骨の世界 × boneInverse」の重みの和だけで決まる（AttachedBindMode）。節点の世界の行列は法線（`(M Mᵀ)⁻¹`）と並びと視錐台に使う。重みは `glb::normalize_skin_weights`（GLTFLoader の normalizeSkinWeights）。動作を替えるときの `mixer.stopAllAction()` は `mixer.stop_all(&mut skel)`（次の動作に無い骨が元の姿へ戻る）。
- 視錐台: 既定は形の球（バインドの姿勢）。three は最初に描くときの姿勢で測るので同じではない。動きで大きく外れる物は `set_skinned_bounds` か `frustum_culled = false`。
- Custom の材質: `vs` は `@vertex fn vs(v: VIn, i: IIn, @location(5) joints: vec4u, @location(6) weights: vec4f)` の形で、`let s = k_skin_matrix(joints, weights)`（世界の位置は `s * vec4f(v.pos, 1.0)`）、`k_skin_normal(inst_model(i), s, v.normal)` が世界の法線。例は aisns の `rust/game/shaders/skinned.wgsl`。`instance_attrs` は使えない（@location(5)・(6) が重なる）。
- 影の型は `cast_shadow` で骨どおりに描く（`vs_shadow_skinned`）。層 1（自機の型・物陰の影絵）には出ない。Custom の材質の `alpha_test` は影の型に効かない（組み込みの材質は `map` があれば効く）。
- モーフ（NPC のまばたき）: `kit.set_morph_weights(model.meshes[k], &[w0, w1, ..])`（three の `morphTargetInfluences`。番号は glTF の targets の順、名前は `g.meshes[..].target_names`）。`instantiate_skinned` がモーフのある部分に差分を持たせる（手で組むなら `kit.set_geometry_morphs(geo, &形, &位置の差分, &法線の差分)`）。重みが変わったコマだけ、CPU で「元 + Σ 重み × 差分」を作って節点ごとの頂点の列へ送り、主の的と影の型がそれを読む（骨は GPU のまま）。重みの既定は全部 0（GLTFLoader が入れる `mesh.weights` の初めの値は入れない。要るならゲームが渡す）。
- GPU の骨は 1 組に 1 つの storage buffer（group 3）で、1 コマ 1 回だけ送る。

### 別のカメラで的へ描く（three の Reflector）

```rust
let rt = kit.render_target(1024, 1024);                    // 起動時に 1 回（HDR・MSAA 4 倍）
// 描く間隔はゲームが決める（three の版の onBeforeRender の 70 ms ごとなど）。呼んだときだけ描く
let view = RawView::from_three(virtual_view, oblique_proj, virtual_pos);  // three の -1..1 の投影（斜めの near 入り）をそのまま
kit.render_view_to(rt, &view, &ViewDraw { hide: &[mirror_node], clear: None, clear_alpha: 0.0 });
// 読む: Custom の材質の textures に rt を入れ、tex0 を textureSample。three の textureMatrix の uv（下が 0）は y を 1 - y に
```

- `RawView { view, proj, position, layers }` の proj は WebGPU の奥行き 0..1。`RawView::from_three` は three の投影の奥行きだけを `z' = (z + w) / 2` で直す（x・y・w と視錐台の 6 面は同じなので、斜めの near の切り方も同じ）。`RawView::from_camera(&camera)` は今の `Camera` から。
- 後処理なし・色は線形のまま（`kit.output` によらない）。MSAA は主の的と同じ 4 倍（kit のパイプラインは全部 4 倍。three の `multisample: 2` も 4 倍で描く。`kit.set_aa` で Off・Smaa にしたときは 1 サンプル）。
- 影の型は描かず、主の描画で前に描いた型とそのときの光の行列を使う（Reflector が `shadowMap.autoUpdate` を切るのと同じ）。
- `hide` の節点（子も）はこの描画だけ隠す（鏡の板・水の入れ物）。`clear` は線形の色（None は主の描画の clear）、`clear_alpha` の既定は 0。

### 透過用の別描画（three の transmission）

```rust
let t = kit.transmission_target();                         // 番号は変わらない。大きさは render が合わせる
kit.transmission = Some(Transmission { scale: 1.0, exclude: vec![glass_enemy_root] });
// 透過する材質（Custom）の textures に t を入れ、textureSampleLevel(tex, smp, uv, lod)。大きさは kit.tex_size(t)（three の transmissionSamplerSize）
```

- `render` が主の的の前に、不透明の列だけを主のカメラで透過用の的（HDR・ミップ付き・MSAA 4 倍で描いて解決）へ描き、16 ビットの浮動小数のミップ（4 点の平均）を同じ command buffer で作る。大きさは主の的 × `scale`（切り捨て、three の `transmissionResolutionScale`）。
- 除くのは `exclude` の節点（子も）と、透過用の的を textures に持つ材質（自分を読みながら描かない）。半透明の列・影絵は描かない。
- 両面（`Side::Double`）の透過する物（透過用の的を textures に持つ材質か `exclude` の節点）は、three と同じく裏の面だけを重ねる: 不透明の列を描いて解決・ミップ → 裏の面（表の面を間引く。材質はこのときの的を読む）→ 解決し直し・ミップを作り直す。半透明の物も裏の面だけ。
- 消す色は three と同じ（clear alpha が 1 未満なら白・0.5、1 なら `RenderOpts.clear`）。three の `scene.background` に当たる物は kit に無い（空は形で描く）。
- 読む uv: `@builtin(position).xy / k_res()` がそのまま的の uv（上が 0）。three の画面の uv（下が 0）を写すなら y を 1 - y に。

### 画像の上げ直し・ミップ

- `kit.write_texture(tex, x, y, w, h, &rgba)`: 全体なら作るときと同じく下の段も作り直す。一部のときは 0 段を書いて、GPU で作れる画像（`set_gpu_mips(true)` のあとに作った物）だけ下の段を作り直す。それ以外の画像の一部の上げ直しは下の段が前のまま（ミップを使わない `TexOpts { mipmap: false, .. }` の画像に使う）。canvas の画像に使うと canvas の中身と食い違う（canvas は `redraw_canvas`）。
- `kit.set_gpu_mips(true)`: このあとに作る `data_texture`・`image_data_texture`・`glb_texture`・`canvas_texture` と、その上げ直し（`write_texture`・`redraw_canvas`）のミップを GPU で作る（`shaders/mips.wgsl`）。sRGB は CPU の式から作った表（線形にする 256 個と、8 ビットへ戻す境目の 255 個）で計算し、GPU の pow を使わないので、CPU の `downsample` と同じ値になる（Mac の Metal で全部の段の全部の画素が一致。`tests/gpu.rs` の `gpu_mips_match_cpu`）。画像は `RENDER_ATTACHMENT`・`COPY_SRC` と、sRGB なら `Rgba8Unorm` の view を持つ。主役（character-view）の画像はこの口を通らない。

### Device を使い回す（場所をまたいで Kit を作り直すゲーム）

```rust
thread_local! { static GPU: RefCell<Option<Gpu>> = RefCell::new(None); }
let gpu = match GPU.with(|g| g.borrow().clone()) { Some(g) => g, None => { let g = Gpu::new().await?; GPU.with(|c| *c.borrow_mut() = Some(g.clone())); g } };
let kit = Kit::with_gpu(canvas, &gpu)?;                  // Kit::new と同じ設定。canvas は前と違ってよい
// 場所を離れるとき（stage.free）: device.destroy() でなく
kit.release_gpu();                                       // この Kit が作った GPU の物を destroy。Device は残る
```

主役を持ち回る（組み立て直さない・GPU へ上げ直さない）:

```rust
let carried: KitActor = kit.take_actor(node).unwrap();   // release_gpu の前に取り出す（GPU の物は残る）
kit.release_gpu();
// 次の場所の Kit（同じ Gpu）。動く部品は組み立てた直後に取った actor.runtime_template() の spawn で作り直すと、組み立てた直後と同じ状態から始まる
let mut carried = carried; carried.actor = Some(template.spawn());
next.put_actor(node2, carried);
// 入れずに捨てるなら carried.destroy_gpu()
```

- `Kit::new` を作り直す形のままにしない理由: Device を作り直すと、`device.destroy()` しても GPU のプロセスに前の分が残った（aisns で約 +170〜190 MB）。`Kit` を作り直さずに中身だけ空にする口より、canvas が場所ごとに替わっても使えて、前の場所の物が残らない（Kit ごと捨てる）ので、こちらにした。
- `release_gpu` が destroy するのは kit が持つ物（テクスチャ・描く的・影の型・後処理の的・形・材質の値と storage・インスタンスと点と線の列・骨・モーフの列）と、入っている主役・NPC の画像と列（`ActorView::destroy_gpu`・`NpcView::destroy_gpu`。同じ形を `share_actor` で使う体どうしは最後の 1 体が返す）。パイプライン・bind group は GC 任せ（GPU のメモリはほとんど持たない）。呼んだあとの Kit は描かない。
- 主役を入れ替える・外す（`set_actor`・`share_actor` で前の主役がいた節点、`clear_actor`、入れ物の節点を消したとき、`set_npc_actor` で主役を NPC に替えたとき）も、前の主役の GPU の物を返す。武器の模型も外す・替えるときに返す（character-view の `clear_weapon`・`set_weapon`）。
- `take_actor`・`put_actor`（`KitActor`）: 同じ `Gpu` で作った Kit どうしで主役を渡す。`put_actor` は輪郭の光の式をその Kit の物にし、その Kit にまだ主役のパイプラインが無ければ持ち回った物を使い回す。見せ方の値（`look`）は持ち回る。武器を持ち越したくなければ `clear_actor_weapon`。
- `Kit::headless(&gpu, w, h)`（Mac だけ）: 画面なしの Kit。仕上げは `kit.headless_output()` の画像へ描く。`tests/gpu.rs` の土台。

### 描く順（render）

1. 影の型（`cast_shadow` の方向の光ごとに 1 枚、2 枚まで。層 0 にある物だけ。光の視錐台で間引く。`shadow_auto_update = false` なら頼まれたコマだけ）
2. outline なら自機の型（層 1。Mesh・InstancedMesh と主役）
3. `kit.transmission` があれば透過用の的（不透明の列だけ）とそのミップ。両面の透過する物があれば、その裏の面を重ねてミップを作り直す
4. 主の的: クリア → 不透明 → 半透明（並びは「決めごと」）→ hidden なら影絵（`kit.hidden_color`・`hidden_opacity`）
5. 保護 → ブルーム → 仕上げ → 画面

光の上限: 方向 4・点 16・スポット 2・半球 2（超えた分は無視）。ゲームの順番は、動きの更新 → `scene` の値 → `actor.update` → `set_final_uniform` → `kit.render`。

### つまずいた所

- `Rc<RefCell<..>>` の kit を `await` をまたいで借りない（読む間に輪が止まる／panic）。読むのは借りずに、終わってから借りて入れる（`hero_web`・`load_image`・`load_glb` は kit を借りない）。
- 的を貼った板は `render_to` の間 `visible = false`（自分を読みながら描くと WebGPU の検証エラーで 1 コマ消える。keiba も隠している）。
- 検証エラーは `shot.py` のコンソールに出る。最初の 1 件が原因で、残りは連鎖。WGSL は `let (a, b) = ..` が書けない、`mat3(m)` は `mat3x3f(m[0].xyz, m[1].xyz, m[2].xyz)`、`vec3 += float` は `vec3f(x)`。
- OffscreenCanvas の文字は、字体を読み終える前に描くと代わりの字体になる（`load_font`）。
- three と同じ `transparent: true` だけでは `depth_write` は true のまま。粒や光は `.see_through()`・`.additive()` か `depth_write: false`。
- Points の `uScale` のように画面の高さから作る値は、窓が変わったらゲームが渡し直す（three と同じ）。
- `smoothstep(1.0, 0.82, x)` のように low > high の定数は WGSL ではシェーダーの作成エラー（GLSL は通る）。`1.0 - smoothstep(0.82, 1.0, x)` と同じ値。`pow(負の数, 2.0)` は NaN になるので `d * d` と書く。
- InstancedMesh の視錐台の判定の球は最初に描くときの行列で決まる（three と同じ）。`set_instance` の前に描くと原点の小さな球になって、原点が画面から外れると消える。最初に描く前に置くか `reset_instance_bounds`。
- 見比べは同じ場面を three でも組むのが早い（`rust/compare/min-three.ts`。今の部品を import する）。粒子のざらつき（`hash` の ±0.012）が sRGB の暗い青で ±5 ほど出るので、差の画像で一面に薄く出るのはそれ。

## 移し方（残りのゲームの担当はこの順で）

pengo が手本。1 本の完了条件は skills の `build-aiarranger-3d-game/references/rust-migration.md`。

| 順 | TS のどのファイル | Rust のどこ | 確かめ方 |
|---|---|---|---|
| 1 | `src/sim/*.ts`（遊びの計算） | `rust/sim/src/*.rs`。式の順番どおり、三角関数は `sim-math` | `rust/sim` のトレース試験（同じ種で TS とビット単位） |
| 2 | `src/view/*.ts`（three の部品） | `rust/game/src/view/*.rs`。1 ファイルずつ同じ名前で。`renderer.ts` の FinalShader は `shaders/final.wgsl`、他の GLSL も `shaders/` へ式の順に | 下の撮り比べ |
| 3 | `src/app.ts` | `rust/game/src/app.rs`（`frame` の順番を崩さない） | `rust/compare/playthrough.py` |
| 4 | `src/ui.ts`・`src/audio.ts`・`src/input.ts`・`src/character-delivery.ts` | `ui.rs`・`audio.rs`・`controls.rs`・`character_delivery.rs` | 通し |
| 5 | `app.ts` の `__game` | `game_api.rs`（同じ名前。読むだけの `sim` は `sim.state.*` を同じ名前で） | 通し・撮り比べ |
| 6 | `index.html`・`src/style.css`・`public/` | 手で写さない。`rust/web/build.sh` が dist へ置いて script の入口だけ差し替える | ブラウザで開く |

### 見比べ（pengo の `rust/compare/` を写して名前を変える）

- `scenes.py`: TS（`npm run dev` の台帳のポート）と Rust（dist の台帳のポート）を並べて、同じコマで止めて撮る。TS の中は変えない。init script で `performance.now`・`requestAnimationFrame`・`Math.random` を差し替え、`__h.step(秒, コマ数)` で両方を手で進める。撮るたびに `sim.state` の一致も見る。左右と差（4 倍）の画像を `<out>/<人>-<場面>-cmp.png` に作るので、**必ず自分の目で見る**。ゲームごとに直すのは場面の並びだけ。
- 乱数はそろわない所がある。TS は three の `generateUUID` が `Math.random` を読むので並びが違う。ボットの種のように遊びに効くものは、そのコマだけ `__h.constant(v)` でそろえる。漂う粒の位置は差に点で残る。
- `perf.py`: 同じ自動プレイで vsync を外した測り方と普通の測り方、読み込み。今の版は `vite build --outDir <外>` して `vite preview --outDir <外> --port <台帳>` で配る。
- 差の画像に残ってよいもの: 粒子のざらつき（`hash` の 1 画素ごとの乱数。暗い色と 1 色が 0 に近い所で強く見える。2 画素ぼかすと消えるので `ぼかした差` の方を読む）、漂う粒の位置、1 画素の線の縁。これ以外は原因を探す。

### 見比べで見つかったこと（three の見えない動き）

- **`computeVertexNormals` は倍精度。** 量子化の位置（Int16 の normalized）を倍精度で割って、外積も倍精度で取り、足すのは Float32Array。f32 で計算すると、髪の細い三角で法線が消えるか別の向きになり、白い筋が出た。`character_actor::glb` は同じ桁にした。
- **両面の半透明は 2 回描く。** `transparent && DoubleSide` は裏の面（`FLIP_SIDED` で法線も裏返す）→ 表の面の順。1 回で描くとまぶたの板の裏が前に出た。`scene` の Mesh・InstancedMesh も主役も ＞＜ の板も同じにした（`forceSinglePass` は持たない）。
- **スキンの法線は `normalMatrix × 骨`。** 入れ物が伸び縮みするとき、骨の世界の行列を法線にそのまま掛けると上の面の光がずれる。`(P Pᵀ)⁻¹` を掛ける（`character_view` の `nfix`）。
- **光線も倍精度。** ＞＜ の板の形を作る光線（`hurt_face.rs`）は三角の計算を倍精度に。f32 だと小さい三角の当たりが変わって板の縁の形が変わる。
- **`vector.project(camera)` は今のカメラを読まない。** 読むのは `camera.matrixWorldInverse` で、描画のときに更新されるほか、`lookAt` の最初の `updateWorldMatrix` でも「新しい位置と lookAt の前の向き」へ更新される。集中線の中心のように毎コマ project する物は、この順に写さないと線がずれる（pinball の `app.rs::cam_look_at`）。
- **`k ** 3` は V8 が OS の pow を呼ぶ。** `libm::pow` とは 10% の値で最後の桁が違う。Mac の試験は `extern "C" pow` で 200 万個まで一致。ブラウザは OS の関数が読めないので倍々の桁で丸めた物（pinball の `sim/game.rs::cube_exact`。違うのは 0.14%）。
- **時計は倍精度で足す。** `time` と `modeT` を f32 で足すと、2 分で 0.03 秒ずれ、時間で回す動き（うれしいくるり）が目に見えて違い、`modeT > 5.7` の境目も 1 コマずれる。`f64` で足して渡すときに `as f32`。境目の定数も `f64` で書く（`2.4f32 as f64` は 2.4 ではない）。

### 道具でつまずいた所

- Playwright の `wait_for_function` の既定は rAF で待つので、rAF を差し替えたページでは止まる。`polling=200` を渡す。
- 手で進めるページは `ctx.close()` が戻らないことがある。自分のプロファイルの Chrome だけ `pkill -f <プロファイル>` で止める。
- CSS の `animation` は本当の時間で進むので、`screenshot(animations='disabled')` で撮る。
- `Math.random` を定数にしたまま three の主役を読むと、`generateUUID` が全部同じ値を返して今の版の主役が白い T ポーズに壊れる（TS 側だけに出る、見比べの仕掛けのせいの絵）。主役を読む間は種のある乱数にして、読んだあとで定数にする。
- `setTimeout` は手で進めていても本当の時間で走る。TS の「あとで出す粒」が撮る時刻で前後する。
- `python3 -m http.server` は Last-Modified だけなので、同じプロファイルだとブラウザがビルド前の wasm を使うことがある（直したのに数字が 1 桁も変わらないときはこれ）。CDP の `Network.setCacheDisabled` で切る（`scenes.py` は切っている）。
- 遊びの刻みの端数（`simAcc`）は左右で揃わない。ボットが 1 コマに 1 回考えるゲームは、端数が違うと遊びが分かれる。時計を同じ値に置いて（`__h.setNow`）止め（hitstop）を踏むと、左右とも端数が 0 になる（pinball の `scenes.py::open_both`）。
- `navigator.getGamepads` の置き換えは、kit が `Gamepad`・`GamepadButton` か確かめるので `Object.setPrototypeOf(pad, Gamepad.prototype)` が要る（pinball の `playthrough.py`）。
- `perf.py` の遅いコマの記録（`slow`: コマ番号・間隔・JS・画面・島）を先に見る。rAF の中の JS が短いのに間隔だけ長いなら、ゲームの処理の外。
- この Mac は Xcode のライセンスが未同意なので `rust/.cargo/config.toml` の `DEVELOPER_DIR` が要る（`cargo test` がリンクで止まるときはこれ）。
