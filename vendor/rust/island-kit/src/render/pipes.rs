// パイプラインの作り置き（必要になったときに作って持つ）。鍵は three の材質の設定と同じ並び。
use super::{Kit, DEPTH, HDR, MASK};
use crate::material::{MatKind, Material, Side};
use wgpu::{CompareFunction, VertexFormat};

/// 独自の属性の @location（character_view の VIn が 0..4、IIn が 8..12 を使う）
pub(crate) const ATTR_LOCS: [u32; 6] = [13, 14, 15, 5, 6, 7];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Pass { Main, Shadow, ShadowAlpha, Mask, Hidden }

/// 頂点の並びの種類
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum VertKind { Mesh, Points, Line, Skinned }

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct PipeKey {
    pub pass: Pass,
    /// 組み込みの番号か、Custom の番号（CUSTOM_BASE 以上）
    pub shader: u32,
    pub vert: VertKind,
    pub blend: Option<wgpu::BlendState>,
    pub cull: Option<wgpu::Face>,
    pub cw: bool,
    pub depth_write: bool,
    pub depth_cmp: CompareFunction,
    pub bias: (u32, u32),
    pub color_write: bool,
    pub attrs: Vec<VertexFormat>,
}

pub(crate) const CUSTOM_BASE: u32 = 1000;

const S_BASIC: u32 = 0;
const S_LAMBERT: u32 = 1;
const S_SOFT: u32 = 2;
const S_PHONG: u32 = 3;
const S_STANDARD: u32 = 4;
const S_SPRITE: u32 = 5;
const S_POINTS: u32 = 6;
const S_LINE: u32 = 7;
const S_PHYSICAL: u32 = 8;

fn builtin_entries(shader: u32) -> (&'static str, &'static str) {
    match shader {
        S_BASIC => ("vs_mesh", "fs_basic"),
        S_LAMBERT => ("vs_mesh", "fs_lambert"),
        S_SOFT => ("vs_mesh", "fs_soft"),
        S_PHONG => ("vs_mesh", "fs_phong"),
        S_STANDARD => ("vs_mesh", "fs_standard"),
        S_SPRITE => ("vs_sprite", "fs_sprite"),
        S_POINTS => ("vs_points", "fs_basic"),
        S_LINE => ("vs_line", "fs_basic"),
        S_PHYSICAL => ("vs_mesh", "fs_physical"),
        _ => unreachable!(),
    }
}

/// Custom のシェーダー（同じ本文は 1 回だけ作る）
pub(crate) struct CustomEntry { pub module: wgpu::ShaderModule, pub vs: String, pub fs: String, pub label: String }

/// パイプラインの確かめの状態（Custom のシェーダーの作成エラーを材質ごとに閉じ込める。kit.take_shader_errors）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PipeState { Ok, Pending, Failed }

/// エラーの範囲（push_error_scope）の結果。何を確かめたか
#[derive(Clone, Copy, Debug)]
pub(crate) enum CheckOf { Module(u32), Pipe(u32) }

/// 確かめの結果の受け口（ブラウザでは結果があとのコマで届く）
pub(crate) type CheckSink = std::sync::Arc<std::sync::Mutex<Vec<(CheckOf, Option<String>)>>>;

/// エラーの範囲を閉じて、結果を sink へ（ブラウザは spawn_local で待つ。Mac の wgpu はその場で出る）
pub(crate) fn watch_scope(scope: wgpu::ErrorScopeGuard, what: CheckOf, sink: &CheckSink) {
    let fut = scope.pop();
    let sink = sink.clone();
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let e = fut.await;
        if let Ok(mut s) = sink.lock() { s.push((what, e.map(|e| e.to_string()))); }
    });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut fut = std::pin::pin!(fut);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        if let std::task::Poll::Ready(e) = fut.as_mut().poll(&mut cx) && let Ok(mut s) = sink.lock() { s.push((what, e.map(|e| e.to_string()))); }
    }
}

/// 面の向きと面のかき分け（three の setMaterial と frontFaceCW）。shadow は影の型（three の shadowSide）
pub(crate) fn cull_of(side: Side, shadow: bool) -> Option<wgpu::Face> {
    match (side, shadow) {
        (Side::Double, _) => None,
        (Side::Front, false) | (Side::Back, true) => Some(wgpu::Face::Back),
        (Side::Back, false) | (Side::Front, true) => Some(wgpu::Face::Front),
    }
}

impl Kit {
    fn shader_of(&mut self, m: &Material) -> u32 {
        match &m.kind {
            MatKind::Basic => S_BASIC,
            MatKind::Lambert(_) => S_LAMBERT,
            MatKind::Soft(_) => S_SOFT,
            MatKind::Phong(_) => S_PHONG,
            MatKind::Standard(_) => S_STANDARD,
            MatKind::Physical(_) => S_PHYSICAL,
            MatKind::Sprite { .. } => S_SPRITE,
            MatKind::Points { .. } => S_POINTS,
            MatKind::LineBasic => S_LINE,
            MatKind::Custom(c) => {
                let key = (c.wgsl.clone(), c.vs.clone(), c.fs.clone());
                if let Some(id) = self.custom_ids.get(&key) { return *id; }
                let id = CUSTOM_BASE + self.customs.len() as u32;
                let module = self.custom_module(&c.label, &c.wgsl, id);
                self.customs.push(CustomEntry { module, vs: c.vs.clone(), fs: c.fs.clone(), label: c.label.clone() });
                self.custom_ids.insert(key, id);
                id
            }
        }
    }

    fn custom_module(&mut self, label: &str, wgsl: &str, id: u32) -> wgpu::ShaderModule {
        if let Some(m) = self.custom_modules.get(wgsl) { return m.clone(); }
        let code = format!("{}\n{}\n{}", character_view::COMMON, super::KIT_COMMON, wgsl);
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let m = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(code.into()) });
        watch_scope(scope, CheckOf::Module(id), &self.shader_sink);
        self.custom_modules.insert(wgsl.to_string(), m.clone());
        m
    }

    /// 主の的のパイプライン（材質と、節点の種類と、裏返しか）
    pub(crate) fn main_pipe(&mut self, mat: u32, vert: VertKind, cw: bool) -> u32 {
        let slot = vert_slot(vert, cw);
        let cached = self.mats[mat as usize].pipes[slot];
        if cached != u32::MAX { return cached; }
        let m = self.mats[mat as usize].m.clone();
        let shader = self.shader_of(&m);
        let attrs = match (&m.kind, vert) {
            (MatKind::Custom(c), VertKind::Points) => c.point_attrs.clone(),
            (MatKind::Custom(c), VertKind::Mesh) => c.instance_attrs.clone(),
            _ => Vec::new(),
        };
        let key = PipeKey {
            pass: Pass::Main, shader, vert,
            blend: m.blend_state(), cull: cull_of(m.side, false), cw,
            depth_write: m.depth_write && m.depth_test,
            depth_cmp: if m.depth_test { m.depth_func } else { CompareFunction::Always },
            bias: if vert == VertKind::Line { (0, 0) } else { m.polygon_offset.map_or((0, 0), |(f, u)| (f.to_bits(), u.to_bits())) },
            color_write: m.color_write,
            attrs,
        };
        let id = self.pipe(key);
        self.mats[mat as usize].pipes[slot] = id;
        id
    }

    /// 両面の半透明の 2 回描き（three の renderObject: 裏の面を先に描いてから表の面）。face は間引く面。Mesh・InstancedMesh だけ
    pub(crate) fn side_pipe(&mut self, mat: u32, vert: VertKind, face: wgpu::Face, cw: bool) -> u32 {
        let slot = (vert == VertKind::Skinned) as usize * 4 + (face == wgpu::Face::Back) as usize * 2 + cw as usize;
        let cached = self.mats[mat as usize].side_pipes[slot];
        if cached != u32::MAX { return cached; }
        let m = self.mats[mat as usize].m.clone();
        let shader = self.shader_of(&m);
        let attrs = match &m.kind { MatKind::Custom(c) if vert == VertKind::Mesh => c.instance_attrs.clone(), _ => Vec::new() };
        let key = PipeKey {
            pass: Pass::Main, shader, vert,
            blend: m.blend_state(), cull: Some(face), cw,
            depth_write: m.depth_write && m.depth_test,
            depth_cmp: if m.depth_test { m.depth_func } else { CompareFunction::Always },
            bias: m.polygon_offset.map_or((0, 0), |(f, u)| (f.to_bits(), u.to_bits())),
            color_write: m.color_write,
            attrs,
        };
        let id = self.pipe(key);
        self.mats[mat as usize].side_pipes[slot] = id;
        id
    }

    pub(crate) fn plain_pipe(&mut self, pass: Pass, vert: VertKind, side: Side, cw: bool) -> u32 {
        let (cull, depth_write, depth_cmp, blend) = match pass {
            Pass::Shadow | Pass::ShadowAlpha => (cull_of(side, true), true, CompareFunction::LessEqual, None),
            Pass::Mask => (Some(wgpu::Face::Back), true, CompareFunction::LessEqual, None),
            Pass::Hidden => (Some(wgpu::Face::Back), false, CompareFunction::Greater, Some(character_view::BLEND_NORMAL)),
            Pass::Main => unreachable!(),
        };
        self.pipe(PipeKey { pass, shader: 0, vert, blend, cull, cw, depth_write, depth_cmp, bias: (0, 0), color_write: true, attrs: Vec::new() })
    }

    fn pipe(&mut self, key: PipeKey) -> u32 {
        if let Some(i) = self.pipe_ids.get(&key) { return *i; }
        let id = self.pipes.len() as u32;
        // Custom のシェーダーのパイプラインはエラーの範囲で作り、作れなかった物は描かない（そのコマの command buffer ごと無効にしない）
        let custom = key.pass == Pass::Main && key.shader >= CUSTOM_BASE;
        let scope = custom.then(|| self.device.push_error_scope(wgpu::ErrorFilter::Validation));
        let p = self.build_pipe(&key);
        let failed_module = custom && self.failed_modules.contains_key(&key.shader);
        self.pipes.push(p);
        self.pipe_state.push(if failed_module { PipeState::Failed } else if custom { PipeState::Pending } else { PipeState::Ok });
        self.pipe_custom.push(if custom { key.shader } else { u32::MAX });
        self.pipe_ids.insert(key, id);
        if let Some(scope) = scope {
            watch_scope(scope, CheckOf::Pipe(id), &self.shader_sink);
            // Mac の wgpu は結果がその場で届くので、このコマから描かない（ブラウザはあとのコマで届く）
            self.drain_shader_checks();
        }
        id
    }

    /// 届いた確かめの結果を入れる（render の初め）。作れなかった Custom のシェーダーは、ブラウザのコンソールへ出して take_shader_errors に積む
    pub(crate) fn drain_shader_checks(&mut self) {
        let got: Vec<(CheckOf, Option<String>)> = match self.shader_sink.lock() { Ok(mut s) => std::mem::take(&mut *s), Err(_) => return };
        for (what, err) in got {
            match what {
                CheckOf::Module(id) => if let Some(e) = err { self.failed_modules.insert(id, e); },
                CheckOf::Pipe(p) => {
                    let p = p as usize;
                    let Some(e) = err else {
                        if self.pipe_state[p] == PipeState::Pending { self.pipe_state[p] = PipeState::Ok; }
                        continue;
                    };
                    self.pipe_state[p] = PipeState::Failed;
                    let c = self.pipe_custom[p];
                    if !self.reported.insert(c) { continue; }
                    let label = self.customs.get((c - CUSTOM_BASE) as usize).map_or("custom", |x| x.label.as_str());
                    let msg = self.failed_modules.get(&c).unwrap_or(&e);
                    let text = format!("island-kit: 材質「{label}」のシェーダーを作れないので描かない: {msg}");
                    #[cfg(target_arch = "wasm32")]
                    web_sys::console::error_1(&text.clone().into());
                    self.shader_errors.push(text);
                }
            }
        }
    }

    /// このパイプラインで描いてよいか（作れなかった物は描かない。set_wait_for_shader_check なら確かめ終わるまで描かない）
    pub(crate) fn pipe_ready(&self, pipe: u32) -> bool {
        match self.pipe_state[pipe as usize] { PipeState::Ok => true, PipeState::Failed => false, PipeState::Pending => !self.wait_shader_check }
    }

    fn build_pipe(&self, k: &PipeKey) -> wgpu::RenderPipeline {
        let inst_attrs = wgpu::vertex_attr_array![8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4];
        let pos_attr = wgpu::vertex_attr_array![0 => Float32x3];
        let extra: Vec<[wgpu::VertexAttribute; 1]> = k.attrs.iter().enumerate().map(|(i, f)| [wgpu::VertexAttribute { format: *f, offset: 0, shader_location: ATTR_LOCS[i] }]).collect();
        let mut vbs: Vec<Option<wgpu::VertexBufferLayout>> = match k.vert {
            VertKind::Mesh => vec![
                Some(character_view::vert_layout()),
                Some(wgpu::VertexBufferLayout { array_stride: 80, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs }),
            ],
            // 形・世界の行列・JOINTS_0 と WEIGHTS_0（@location(5)・(6)。Custom の instance_attrs は持たない）
            VertKind::Skinned => vec![
                Some(character_view::vert_layout()),
                Some(wgpu::VertexBufferLayout { array_stride: 80, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs }),
                Some(character_view::skin_layout()),
            ],
            // 点ごとの板: 位置はインスタンスの流れ、世界の行列は全部の点で同じ（stride 0）
            VertKind::Points => vec![
                Some(wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Instance, attributes: &pos_attr }),
                Some(wgpu::VertexBufferLayout { array_stride: 0, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs }),
            ],
            VertKind::Line => vec![
                Some(wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &pos_attr }),
                Some(wgpu::VertexBufferLayout { array_stride: 80, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs }),
            ],
        };
        for (i, a) in extra.iter().enumerate() {
            vbs.push(Some(wgpu::VertexBufferLayout { array_stride: k.attrs[i].size(), step_mode: wgpu::VertexStepMode::Instance, attributes: a }));
        }
        let skinned = k.vert == VertKind::Skinned;
        let (module, vs, fs): (&wgpu::ShaderModule, &str, Option<&str>) = match k.pass {
            Pass::Shadow => (&self.kit_shader, if skinned { "vs_shadow_skinned" } else { "vs_shadow" }, None),
            Pass::ShadowAlpha => (&self.kit_shader, if skinned { "vs_shadow_alpha_skinned" } else { "vs_shadow_alpha" }, Some("fs_shadow_alpha")),
            Pass::Mask => (&self.kit_shader, "vs_mask", Some("fs_mask")),
            Pass::Hidden => (&self.kit_shader, "vs_hidden", Some("fs_hidden")),
            Pass::Main if k.shader >= CUSTOM_BASE => {
                let c = &self.customs[(k.shader - CUSTOM_BASE) as usize];
                (&c.module, c.vs.as_str(), Some(c.fs.as_str()))
            }
            Pass::Main => { let (v, f) = builtin_entries(k.shader); (&self.kit_shader, if skinned { "vs_mesh_skinned" } else { v }, Some(f)) }
        };
        let (target, samples) = match k.pass {
            Pass::Main | Pass::Hidden => (Some(HDR), self.samples),
            Pass::Mask => (Some(MASK), 1),
            Pass::Shadow | Pass::ShadowAlpha => (None, 1),
        };
        let layout = match (k.pass, skinned) {
            (Pass::Main, false) => &self.layout_main,
            (Pass::Main, true) => &self.layout_main_skinned,
            (Pass::Shadow, false) | (Pass::Mask, _) => &self.layout_globals,
            (Pass::Shadow, true) => &self.layout_depth_skinned,
            (Pass::ShadowAlpha, false) => &self.layout_shadow_alpha,
            (Pass::ShadowAlpha, true) => &self.layout_shadow_alpha_skinned,
            (Pass::Hidden, _) => &self.layout_hidden,
        };
        let targets: Vec<Option<wgpu::ColorTargetState>> = target.map(|format| wgpu::ColorTargetState {
            format, blend: k.blend, write_mask: if k.color_write { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() },
        }).into_iter().map(Some).collect();
        let (factor, units) = (f32::from_bits(k.bias.0), f32::from_bits(k.bias.1));
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kit"),
            layout: Some(layout),
            vertex: wgpu::VertexState { module, entry_point: Some(vs), buffers: &vbs, compilation_options: Default::default() },
            fragment: fs.map(|fs| wgpu::FragmentState { module, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState {
                topology: if k.vert == VertKind::Line { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
                front_face: if k.cw { wgpu::FrontFace::Cw } else { wgpu::FrontFace::Ccw },
                cull_mode: k.cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH, depth_write_enabled: Some(k.depth_write), depth_compare: Some(k.depth_cmp), stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: units as i32, slope_scale: factor, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        })
    }
}

pub(crate) fn vert_slot(v: VertKind, cw: bool) -> usize {
    (match v { VertKind::Mesh => 0, VertKind::Points => 2, VertKind::Line => 4, VertKind::Skinned => 6 }) + cw as usize
}
