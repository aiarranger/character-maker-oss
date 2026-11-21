// 凍結: character-actor 0.1.14（2026-11-12）の src/glb.rs の写し。NPC・汎用（character-runtime 0.2.5 の写しの決まり）だけが使う。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）
// GLB の読み込み（three の GLTFLoader の役）。meshopt（EXT_meshopt_compression）・webp・骨・動作・モーフを読む。
// gltf クレートは meshopt の bufferView を読めないので、JSON を直接読む。
use glam::{Mat4, Quat, Vec3};
use meshopt_rs::index::buffer::decode_index_buffer;
use meshopt_rs::index::sequence::decode_index_sequence;
use meshopt_rs::vertex::buffer::decode_vertex_buffer;
use meshopt_rs::vertex::filter::{decode_filter_exp, decode_filter_oct_16, decode_filter_oct_8, decode_filter_quat};
use serde_json::Value;

#[derive(Default, Clone)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// 頂点色（無ければ空）
    pub colors: Vec<[f32; 4]>,
    /// aisns の草木の揺れ（_SWAY）
    pub sway: Vec<f32>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
    /// モーフの差分（位置・法線）
    pub morph_pos: Vec<Vec<[f32; 3]>>,
    pub morph_nor: Vec<Vec<[f32; 3]>>,
}

#[derive(Clone)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
    pub target_names: Vec<String>,
    pub weights: Vec<f32>,
}

#[derive(Clone)]
pub struct Node {
    pub name: String,
    pub t: Vec3,
    pub r: Quat,
    pub s: Vec3,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub extras: Value,
    /// EXT_mesh_gpu_instancing の行列（three の InstancedMesh。無ければ空）
    pub instances: Vec<Mat4>,
}

impl Node {
    pub fn local(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.s, self.r, self.t)
    }
}

#[derive(Clone)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Interp { Linear, Step, Cubic }
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Path { Translation, Rotation, Scale, Weights }

#[derive(Clone)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub interp: Interp,
    pub times: Vec<f32>,
    /// path が Rotation なら 4 個ずつ、Translation・Scale は 3 個ずつ、Weights はモーフの数ずつ
    pub values: Vec<f32>,
}

#[derive(Clone)]
pub struct Animation {
    pub name: String,
    pub channels: Vec<Channel>,
    pub duration: f32,
}

#[derive(Clone)]
pub struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    pub texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub emissive_texture: Option<usize>,
    pub double_sided: bool,
    pub alpha_blend: bool,
    pub alpha_cutoff: Option<f32>,
    pub extras: Value,
    /// KHR_materials_transmission・volume・ior（どれかがあれば three の GLTFLoader は MeshPhysicalMaterial にする）。無ければ None
    pub transmission: Option<Transmission>,
}

/// three の GLTFLoader が KHR_materials_transmission・KHR_materials_volume・KHR_materials_ior から MeshPhysicalMaterial に書く値。
/// 書かれていない物は MeshPhysicalMaterial の既定（transmission 0・thickness 0・attenuationDistance 無限・attenuationColor 白・ior 1.5）。
/// テクスチャはほかと同じく画像の番号。thickness_texture は g の値を使う（three の thicknessMap）
#[derive(Clone, Debug, PartialEq)]
pub struct Transmission {
    pub transmission: f32,
    pub transmission_texture: Option<usize>,
    pub thickness: f32,
    pub thickness_texture: Option<usize>,
    pub attenuation_distance: f32,
    /// 線形の色（three は LinearSRGBColorSpace で読む）
    pub attenuation_color: [f32; 3],
    pub ior: f32,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Glb {
    pub json: Value,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
    pub materials: Vec<Material>,
    /// テクスチャ番号 → 画像
    pub images: Vec<Image>,
    pub scene: Vec<usize>,
}

struct Buffers<'a> {
    json: &'a Value,
    bin: &'a [u8],
    /// meshopt を解いた bufferView
    decoded: Vec<Option<Vec<u8>>>,
}

fn f(v: &Value, k: &str, d: f64) -> f64 { v.get(k).and_then(Value::as_f64).unwrap_or(d) }
fn u(v: &Value, k: &str, d: usize) -> usize { v.get(k).and_then(Value::as_u64).map(|x| x as usize).unwrap_or(d) }
fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] { v.get(k).and_then(Value::as_array).map(|a| a.as_slice()).unwrap_or(&[]) }

fn decode_stride<const N: usize>(n: usize, src: &[u8]) -> Result<Vec<u8>, String> {
    let mut d = vec![[0u8; N]; n];
    decode_vertex_buffer(&mut d, src).map_err(|e| format!("meshopt 頂点: {e:?}"))?;
    Ok(bytemuck::cast_slice::<[u8; N], u8>(&d).to_vec())
}

fn meshopt(ext: &Value, bin: &[u8]) -> Result<Vec<u8>, String> {
    let off = u(ext, "byteOffset", 0);
    let len = u(ext, "byteLength", 0);
    let stride = u(ext, "byteStride", 0);
    let count = u(ext, "count", 0);
    let mode = ext.get("mode").and_then(Value::as_str).unwrap_or("ATTRIBUTES");
    let filter = ext.get("filter").and_then(Value::as_str).unwrap_or("NONE");
    let src = &bin[off..off + len];
    let mut out = match mode {
        "ATTRIBUTES" => match stride {
            4 => decode_stride::<4>(count, src)?,
            8 => decode_stride::<8>(count, src)?,
            12 => decode_stride::<12>(count, src)?,
            16 => decode_stride::<16>(count, src)?,
            20 => decode_stride::<20>(count, src)?,
            24 => decode_stride::<24>(count, src)?,
            28 => decode_stride::<28>(count, src)?,
            32 => decode_stride::<32>(count, src)?,
            36 => decode_stride::<36>(count, src)?,
            40 => decode_stride::<40>(count, src)?,
            48 => decode_stride::<48>(count, src)?,
            64 => decode_stride::<64>(count, src)?,
            _ => return Err(format!("meshopt の幅 {stride} は未対応")),
        },
        "TRIANGLES" | "INDICES" => {
            let triangles = mode == "TRIANGLES";
            let mut d = vec![0u32; count];
            if triangles { decode_index_buffer(&mut d, src) } else { decode_index_sequence(&mut d, src) }.map_err(|e| format!("meshopt 添字: {e:?}"))?;
            if stride == 2 { d.iter().flat_map(|i| (*i as u16).to_le_bytes()).collect() } else { bytemuck::cast_slice::<u32, u8>(&d).to_vec() }
        }
        _ => return Err(format!("meshopt の mode {mode}")),
    };
    match filter {
        "NONE" => {}
        "OCTAHEDRAL" => {
            if stride == 4 {
                let mut d = vec![[0u8; 4]; count];
                bytemuck::cast_slice_mut::<[u8; 4], u8>(&mut d).copy_from_slice(&out);
                decode_filter_oct_8(&mut d);
                out = bytemuck::cast_slice::<[u8; 4], u8>(&d).to_vec();
            } else {
                let mut d = vec![[0u16; 4]; count];
                bytemuck::cast_slice_mut::<[u16; 4], u8>(&mut d).copy_from_slice(&out);
                decode_filter_oct_16(&mut d);
                out = bytemuck::cast_slice::<[u16; 4], u8>(&d).to_vec();
            }
        }
        "QUATERNION" => {
            let mut d = vec![[0u16; 4]; count];
            bytemuck::cast_slice_mut::<[u16; 4], u8>(&mut d).copy_from_slice(&out);
            decode_filter_quat(&mut d);
            out = bytemuck::cast_slice::<[u16; 4], u8>(&d).to_vec();
        }
        "EXPONENTIAL" => {
            let mut d = vec![0u32; count * stride / 4];
            bytemuck::cast_slice_mut::<u32, u8>(&mut d).copy_from_slice(&out);
            decode_filter_exp(&mut d);
            out = bytemuck::cast_slice::<u32, u8>(&d).to_vec();
        }
        _ => return Err(format!("meshopt の filter {filter}")),
    }
    Ok(out)
}

impl<'a> Buffers<'a> {
    fn new(json: &'a Value, bin: &'a [u8]) -> Result<Self, String> {
        let views = arr(json, "bufferViews");
        let mut decoded = vec![None; views.len()];
        for (i, v) in views.iter().enumerate() {
            if let Some(ext) = v.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")) {
                decoded[i] = Some(meshopt(ext, bin)?);
            }
        }
        Ok(Buffers { json, bin, decoded })
    }

    fn view(&self, i: usize) -> (&[u8], usize) {
        let v = &arr(self.json, "bufferViews")[i];
        if let Some(d) = &self.decoded[i] {
            let stride = v.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")).map(|e| u(e, "byteStride", 0)).unwrap_or(0);
            return (d, stride);
        }
        let off = u(v, "byteOffset", 0);
        let len = u(v, "byteLength", 0);
        (&self.bin[off..off + len], u(v, "byteStride", 0))
    }

    /// アクセサを f32 の列へ（正規化の整数は 0..1 / -1..1 へ直す）。返すのは 1 要素の個数と値
    fn floats(&self, idx: usize) -> (usize, Vec<f32>) {
        let a = &arr(self.json, "accessors")[idx];
        let comps = comp_count(a);
        let ct = u(a, "componentType", 5126);
        let count = u(a, "count", 0);
        let norm = a.get("normalized").and_then(Value::as_bool).unwrap_or(false);
        let mut out = vec![0f32; count * comps];
        if let Some(view) = a.get("bufferView").and_then(Value::as_u64) {
            let (d, st) = self.view(view as usize);
            let off = u(a, "byteOffset", 0);
            let size = comp_size(ct);
            let stride = if st > 0 { st } else { comps * size };
            for i in 0..count {
                for c in 0..comps {
                    out[i * comps + c] = read_comp(d, off + i * stride + c * size, ct, norm);
                }
            }
        }
        if let Some(sp) = a.get("sparse") {
            let n = u(sp, "count", 0);
            let ii = &sp["indices"];
            let iv = &sp["values"];
            let (d, _) = self.view(u(ii, "bufferView", 0));
            let its = u(ii, "componentType", 5125);
            let (vd, _) = self.view(u(iv, "bufferView", 0));
            let vsize = comp_size(ct);
            for k in 0..n {
                let at = u(ii, "byteOffset", 0) + k * comp_size(its);
                let i = read_comp(d, at, its, false) as usize;
                for c in 0..comps {
                    out[i * comps + c] = read_comp(vd, u(iv, "byteOffset", 0) + (k * comps + c) * vsize, ct, norm);
                }
            }
        }
        (comps, out)
    }

    /// three の getX と同じ値（量子化の整数は倍精度で割る）。疎な差分は使わない形だけ
    fn doubles3(&self, idx: usize) -> Vec<[f64; 3]> {
        let a = &arr(self.json, "accessors")[idx];
        let ct = u(a, "componentType", 5126);
        let count = u(a, "count", 0);
        let norm = a.get("normalized").and_then(Value::as_bool).unwrap_or(false);
        let Some(view) = a.get("bufferView").and_then(Value::as_u64) else { return vec![[0.0; 3]; count] };
        let (d, st) = self.view(view as usize);
        let off = u(a, "byteOffset", 0);
        let size = comp_size(ct);
        let stride = if st > 0 { st } else { 3 * size };
        let one = |at: usize| -> f64 {
            match ct {
                5120 => { let v = d[at] as i8 as f64; if norm { (v / 127.0).max(-1.0) } else { v } }
                5121 => { let v = d[at] as f64; if norm { v / 255.0 } else { v } }
                5122 => { let v = i16::from_le_bytes([d[at], d[at + 1]]) as f64; if norm { (v / 32767.0).max(-1.0) } else { v } }
                5123 => { let v = u16::from_le_bytes([d[at], d[at + 1]]) as f64; if norm { v / 65535.0 } else { v } }
                5125 => u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as f64,
                _ => f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as f64,
            }
        };
        (0..count).map(|i| { let at = off + i * stride; [one(at), one(at + size), one(at + 2 * size)] }).collect()
    }

    fn ints(&self, idx: usize) -> Vec<u32> {
        let a = &arr(self.json, "accessors")[idx];
        let comps = comp_count(a);
        let ct = u(a, "componentType", 5125);
        let count = u(a, "count", 0);
        let (d, st) = self.view(u(a, "bufferView", 0));
        let off = u(a, "byteOffset", 0);
        let size = comp_size(ct);
        let stride = if st > 0 { st } else { comps * size };
        let mut out = Vec::with_capacity(count * comps);
        for i in 0..count {
            for c in 0..comps {
                out.push(read_comp(d, off + i * stride + c * size, ct, false) as u32);
            }
        }
        out
    }
}

fn comp_count(a: &Value) -> usize {
    match a.get("type").and_then(Value::as_str).unwrap_or("SCALAR") {
        "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, "MAT2" => 4, "MAT3" => 9, "MAT4" => 16, _ => 1,
    }
}
fn comp_size(ct: usize) -> usize { match ct { 5120 | 5121 => 1, 5122 | 5123 => 2, _ => 4 } }
fn read_comp(d: &[u8], at: usize, ct: usize, norm: bool) -> f32 {
    match ct {
        5120 => { let v = d[at] as i8 as f32; if norm { (v / 127.0).max(-1.0) } else { v } }
        5121 => { let v = d[at] as f32; if norm { v / 255.0 } else { v } }
        5122 => { let v = i16::from_le_bytes([d[at], d[at + 1]]) as f32; if norm { (v / 32767.0).max(-1.0) } else { v } }
        5123 => { let v = u16::from_le_bytes([d[at], d[at + 1]]) as f32; if norm { v / 65535.0 } else { v } }
        5125 => u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as f32,
        _ => f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]),
    }
}

fn v3s(c: usize, v: Vec<f32>) -> Vec<[f32; 3]> { let _ = c; v.chunks_exact(3).map(|x| [x[0], x[1], x[2]]).collect() }

/// 法線が無い面の法線を、面の向きから作る（three の computeVertexNormals）
pub fn compute_normals(p: &mut Primitive) {
    let pos: Vec<[f64; 3]> = p.positions.iter().map(|q| [q[0] as f64, q[1] as f64, q[2] as f64]).collect();
    compute_normals_from(p, &pos);
}

/// pos は three の fromBufferAttribute が読む値（量子化の整数なら倍精度で割った値）。
/// 外積と長さは倍精度、足した値は Float32Array（f32）。f32 で計算すると、ごく小さい三角が丸めで 0 や別の向きになる
fn compute_normals_from(p: &mut Primitive, pos: &[[f64; 3]]) {
    let mut n = vec![[0f32; 3]; pos.len()];
    let v = |i: usize| pos[i];
    for t in p.indices.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let (pa, pb, pc) = (v(a), v(b), v(c));
        let (cb, ab) = ([pc[0] - pb[0], pc[1] - pb[1], pc[2] - pb[2]], [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]]);
        let f = [cb[1] * ab[2] - cb[2] * ab[1], cb[2] * ab[0] - cb[0] * ab[2], cb[0] * ab[1] - cb[1] * ab[0]];
        for i in [a, b, c] { for k in 0..3 { n[i][k] = (n[i][k] as f64 + f[k]) as f32; } }
    }
    p.normals = n.iter().map(|q| {
        let (x, y, z) = (q[0] as f64, q[1] as f64, q[2] as f64);
        let l = (x * x + y * y + z * z).sqrt();
        let l = if l == 0.0 { 1.0 } else { l };
        [(x / l) as f32, (y / l) as f32, (z / l) as f32]
    }).collect();
}

#[cfg(test)]
mod normals_test {
    use super::*;
    #[test]
    fn tiny_triangles_keep_their_normal() {
        // 外積が 1e-21 ほどの三角（f32 で長さの 2 乗を取ると 0 になる）
        let mut p = Primitive { positions: vec![[0.0, 0.0, 0.0], [1e-10, 0.0, 0.0], [0.0, 1e-11, 0.0]], indices: vec![0, 1, 2], ..Default::default() };
        compute_normals(&mut p);
        for q in &p.normals { assert!((q[2] - 1.0).abs() < 1e-6, "{q:?}"); }
    }
}

pub fn parse(bytes: &[u8]) -> Result<Glb, String> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" { return Err("GLB ではない".into()); }
    let jlen = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json: Value = serde_json::from_slice(&bytes[20..20 + jlen]).map_err(|e| e.to_string())?;
    let bin_at = 20 + jlen;
    let bin: &[u8] = if bytes.len() > bin_at + 8 { &bytes[bin_at + 8..] } else { &[] };
    let b = Buffers::new(&json, bin)?;

    let nodes: Vec<Node> = arr(&json, "nodes").iter().map(|n| {
        let (mut t, mut r, mut s) = (Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
        if let Some(m) = n.get("matrix").and_then(Value::as_array) {
            let m: Vec<f32> = m.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
            let (sc, ro, tr) = Mat4::from_cols_slice(&m).to_scale_rotation_translation();
            t = tr; r = ro; s = sc;
        }
        let v = |k: &str| n.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect::<Vec<f32>>());
        if let Some(x) = v("translation") { t = Vec3::new(x[0], x[1], x[2]); }
        if let Some(x) = v("rotation") { r = Quat::from_xyzw(x[0], x[1], x[2], x[3]); }
        if let Some(x) = v("scale") { s = Vec3::new(x[0], x[1], x[2]); }
        Node {
            name: n.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            t, r, s,
            children: arr(n, "children").iter().filter_map(|c| c.as_u64().map(|c| c as usize)).collect(),
            mesh: n.get("mesh").and_then(Value::as_u64).map(|x| x as usize),
            skin: n.get("skin").and_then(Value::as_u64).map(|x| x as usize),
            extras: n.get("extras").cloned().unwrap_or(Value::Null),
            instances: read_instances(&b, n),
        }
    }).collect();

    let meshes: Vec<Mesh> = arr(&json, "meshes").iter().map(|m| {
        let primitives = arr(m, "primitives").iter().map(|pr| read_primitive(&b, pr)).collect();
        Mesh {
            name: m.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            primitives,
            target_names: m.get("extras").and_then(|e| e.get("targetNames")).and_then(Value::as_array)
                .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default(),
            weights: arr(m, "weights").iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect(),
        }
    }).collect();

    let skins = arr(&json, "skins").iter().map(|s| {
        let joints: Vec<usize> = arr(s, "joints").iter().filter_map(|j| j.as_u64().map(|x| x as usize)).collect();
        let inverse_bind = s.get("inverseBindMatrices").and_then(Value::as_u64).map(|i| {
            let (_, v) = b.floats(i as usize);
            v.chunks_exact(16).map(Mat4::from_cols_slice).collect()
        }).unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
        Skin { joints, inverse_bind }
    }).collect();

    let animations = arr(&json, "animations").iter().map(|a| {
        let samplers = arr(a, "samplers");
        let mut duration = 0f32;
        let channels: Vec<Channel> = arr(a, "channels").iter().filter_map(|c| {
            let target = c.get("target")?;
            let node = target.get("node")?.as_u64()? as usize;
            let path = match target.get("path")?.as_str()? {
                "translation" => Path::Translation, "rotation" => Path::Rotation, "scale" => Path::Scale, "weights" => Path::Weights, _ => return None,
            };
            let s = &samplers[u(c, "sampler", 0)];
            let (_, times) = b.floats(u(s, "input", 0));
            let (_, values) = b.floats(u(s, "output", 0));
            let interp = match s.get("interpolation").and_then(Value::as_str).unwrap_or("LINEAR") { "STEP" => Interp::Step, "CUBICSPLINE" => Interp::Cubic, _ => Interp::Linear };
            if let Some(t) = times.last() { duration = duration.max(*t); }
            Some(Channel { node, path, interp, times, values })
        }).collect();
        Animation { name: a.get("name").and_then(Value::as_str).unwrap_or("").to_string(), channels, duration }
    }).collect();

    let tex_image = |i: usize| -> Option<usize> {
        let t = arr(&json, "textures").get(i)?;
        t.get("source").and_then(Value::as_u64)
            .or_else(|| t.get("extensions")?.get("EXT_texture_webp")?.get("source")?.as_u64())
            .map(|x| x as usize)
    };
    let materials = arr(&json, "materials").iter().map(|m| {
        let pbr = m.get("pbrMetallicRoughness").cloned().unwrap_or(Value::Null);
        let bc = arr(&pbr, "baseColorFactor");
        let e = arr(m, "emissiveFactor");
        let mode = m.get("alphaMode").and_then(Value::as_str).unwrap_or("OPAQUE");
        Material {
            name: m.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            base_color: if bc.len() == 4 { [bc[0].as_f64().unwrap_or(1.0) as f32, bc[1].as_f64().unwrap_or(1.0) as f32, bc[2].as_f64().unwrap_or(1.0) as f32, bc[3].as_f64().unwrap_or(1.0) as f32] } else { [1.0; 4] },
            texture: pbr.get("baseColorTexture").and_then(|t| t.get("index")).and_then(Value::as_u64).and_then(|i| tex_image(i as usize)),
            metallic: f(&pbr, "metallicFactor", 1.0) as f32,
            roughness: f(&pbr, "roughnessFactor", 1.0) as f32,
            emissive: if e.len() == 3 { [e[0].as_f64().unwrap_or(0.0) as f32, e[1].as_f64().unwrap_or(0.0) as f32, e[2].as_f64().unwrap_or(0.0) as f32] } else { [0.0; 3] },
            emissive_texture: m.get("emissiveTexture").and_then(|t| t.get("index")).and_then(Value::as_u64).and_then(|i| tex_image(i as usize)),
            double_sided: m.get("doubleSided").and_then(Value::as_bool).unwrap_or(false),
            alpha_blend: mode == "BLEND",
            alpha_cutoff: if mode == "MASK" { Some(f(m, "alphaCutoff", 0.5) as f32) } else { None },
            extras: m.get("extras").cloned().unwrap_or(Value::Null),
            transmission: read_transmission(m, &tex_image),
        }
    }).collect();

    let mut images = Vec::new();
    for im in arr(&json, "images") {
        let (data, _) = b.view(u(im, "bufferView", 0));
        let dec = image::load_from_memory(data).map_err(|e| format!("画像: {e}"))?.to_rgba8();
        images.push(Image { width: dec.width(), height: dec.height(), rgba: dec.into_raw() });
    }

    let scene_i = u(&json, "scene", 0);
    let scene = arr(&json, "scenes").get(scene_i).map(|s| arr(s, "nodes").iter().filter_map(|n| n.as_u64().map(|x| x as usize)).collect()).unwrap_or_default();
    Ok(Glb { nodes, meshes, skins, animations, materials, images, scene, json: json.clone() })
}

/// GLTFMaterialsTransmissionExtension・GLTFMaterialsVolumeExtension・GLTFMaterialsIorExtension（three r178）の extendMaterialParams と同じ値
fn read_transmission(m: &Value, tex_image: &dyn Fn(usize) -> Option<usize>) -> Option<Transmission> {
    let ext = m.get("extensions")?;
    let (t, v, i) = (ext.get("KHR_materials_transmission"), ext.get("KHR_materials_volume"), ext.get("KHR_materials_ior"));
    if t.is_none() && v.is_none() && i.is_none() { return None; }
    let num = |e: Option<&Value>, k: &str| e.and_then(|e| e.get(k)).and_then(Value::as_f64).map(|x| x as f32);
    let tex = |e: Option<&Value>, k: &str| e.and_then(|e| e.get(k)).and_then(|t| t.get("index")).and_then(Value::as_u64).and_then(|x| tex_image(x as usize));
    let mut out = Transmission { transmission: 0.0, transmission_texture: None, thickness: 0.0, thickness_texture: None, attenuation_distance: f32::INFINITY, attenuation_color: [1.0; 3], ior: 1.5 };
    if t.is_some() {
        if let Some(x) = num(t, "transmissionFactor") { out.transmission = x; }
        out.transmission_texture = tex(t, "transmissionTexture");
    }
    if v.is_some() {
        out.thickness = num(v, "thicknessFactor").unwrap_or(0.0);
        out.thickness_texture = tex(v, "thicknessTexture");
        // attenuationDistance || Infinity（0 も無限）
        out.attenuation_distance = num(v, "attenuationDistance").filter(|d| *d != 0.0).unwrap_or(f32::INFINITY);
        if let Some(c) = v.and_then(|v| v.get("attenuationColor")).and_then(Value::as_array).filter(|c| c.len() >= 3) {
            out.attenuation_color = [c[0].as_f64().unwrap_or(0.0) as f32, c[1].as_f64().unwrap_or(0.0) as f32, c[2].as_f64().unwrap_or(0.0) as f32];
        }
    }
    if i.is_some() { out.ior = num(i, "ior").unwrap_or(1.5); }
    Some(out)
}

/// EXT_mesh_gpu_instancing（GLTFLoader の GLTFMeshGpuInstancing。TRANSLATION・ROTATION・SCALE から行列を作る）
fn read_instances(b: &Buffers, n: &Value) -> Vec<Mat4> {
    let Some(at) = n.get("extensions").and_then(|e| e.get("EXT_mesh_gpu_instancing")).and_then(|e| e.get("attributes")) else { return Vec::new() };
    let acc = |k: &str| at.get(k).and_then(Value::as_u64).map(|x| b.floats(x as usize).1);
    let (t, r, s) = (acc("TRANSLATION"), acc("ROTATION"), acc("SCALE"));
    let count = t.as_ref().map(|v| v.len() / 3).or(r.as_ref().map(|v| v.len() / 4)).or(s.as_ref().map(|v| v.len() / 3)).unwrap_or(0);
    (0..count).map(|i| {
        let tv = t.as_ref().map_or(Vec3::ZERO, |v| Vec3::new(v[i * 3], v[i * 3 + 1], v[i * 3 + 2]));
        let rv = r.as_ref().map_or(Quat::IDENTITY, |v| Quat::from_xyzw(v[i * 4], v[i * 4 + 1], v[i * 4 + 2], v[i * 4 + 3]));
        let sv = s.as_ref().map_or(Vec3::ONE, |v| Vec3::new(v[i * 3], v[i * 3 + 1], v[i * 3 + 2]));
        Mat4::from_scale_rotation_translation(sv, rv, tv)
    }).collect()
}

fn read_primitive(b: &Buffers, pr: &Value) -> Primitive {
    let at = &pr["attributes"];
    let acc = |k: &str| at.get(k).and_then(Value::as_u64).map(|x| x as usize);
    let mut p = Primitive { material: pr.get("material").and_then(Value::as_u64).map(|x| x as usize), ..Default::default() };
    if let Some(i) = acc("POSITION") { let (c, v) = b.floats(i); p.positions = v3s(c, v); }
    if let Some(i) = acc("NORMAL") { let (c, v) = b.floats(i); p.normals = v3s(c, v); }
    if let Some(i) = acc("TEXCOORD_0") { let (_, v) = b.floats(i); p.uvs = v.chunks_exact(2).map(|x| [x[0], x[1]]).collect(); }
    if let Some(i) = acc("COLOR_0") {
        let (c, v) = b.floats(i);
        p.colors = if c == 4 { v.chunks_exact(4).map(|x| [x[0], x[1], x[2], x[3]]).collect() } else { v.chunks_exact(3).map(|x| [x[0], x[1], x[2], 1.0]).collect() };
    }
    if let Some(i) = acc("_SWAY").or_else(|| acc("_sway")) { p.sway = b.floats(i).1; }
    if let Some(i) = acc("JOINTS_0") { p.joints = b.ints(i).chunks_exact(4).map(|x| [x[0] as u16, x[1] as u16, x[2] as u16, x[3] as u16]).collect(); }
    if let Some(i) = acc("WEIGHTS_0") { p.weights = b.floats(i).1.chunks_exact(4).map(|x| [x[0], x[1], x[2], x[3]]).collect(); }
    p.indices = match pr.get("indices").and_then(Value::as_u64) {
        Some(i) => b.ints(i as usize),
        None => (0..p.positions.len() as u32).collect(),
    };
    for t in arr(pr, "targets") {
        let n = p.positions.len();
        p.morph_pos.push(t.get("POSITION").and_then(Value::as_u64).map(|i| { let (c, v) = b.floats(i as usize); v3s(c, v) }).unwrap_or_else(|| vec![[0.0; 3]; n]));
        p.morph_nor.push(t.get("NORMAL").and_then(Value::as_u64).map(|i| { let (c, v) = b.floats(i as usize); v3s(c, v) }).unwrap_or_else(|| vec![[0.0; 3]; n]));
    }
    if p.normals.is_empty() {
        match acc("POSITION") {
            Some(i) => { let pos = b.doubles3(i); compute_normals_from(&mut p, &pos); }
            None => compute_normals(&mut p),
        }
    }
    let n = p.positions.len();
    if p.uvs.len() != n { p.uvs = vec![[0.0, 0.0]; n]; }
    p
}

impl Glb {
    /// ノードの世界行列（シーンの根から）
    pub fn world_matrices(&self) -> Vec<Mat4> {
        let mut w = vec![Mat4::IDENTITY; self.nodes.len()];
        fn go(g: &Glb, i: usize, parent: Mat4, w: &mut [Mat4]) {
            w[i] = parent * g.nodes[i].local();
            for &c in &g.nodes[i].children { go(g, c, w[i], w); }
        }
        for &r in &self.scene { go(self, r, Mat4::IDENTITY, &mut w); }
        w
    }
}
