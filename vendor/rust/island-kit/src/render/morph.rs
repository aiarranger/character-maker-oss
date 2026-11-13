// 骨付きの部分（scene.skinned）のモーフ（three の morphTargetInfluences）。重みが 0 でない節点だけ、CPU で
// 「元の頂点 + Σ 重み × 差分」（位置と法線。glTF の morphTargetsRelative。three のシェーダーと同じ順に足す）を作り、
// 節点ごとの頂点の列へ上げて、骨付きの描画（主の的・影の型）がその列を読む。骨の計算はそのまま GPU。
use super::Kit;
use crate::material::GeoId;
use crate::scene::NodeId;
use character_view::Vertex;

/// 形のモーフの元（kit.set_geometry_morphs）
pub(crate) struct MorphSrc { base: Vec<Vertex>, pos: Vec<Vec<[f32; 3]>>, nor: Vec<Vec<[f32; 3]>> }

/// 節点ごとの重みと、変えた頂点の列
pub(crate) struct MorphNode { uid: u32, weights: Vec<f32>, dirty: bool, buf: Option<wgpu::Buffer> }

/// 元の頂点に重みの分の差分を足す（three の morphtarget_vertex・morphnormal_vertex: transformed += morphTarget * influence を順に）
pub(crate) fn apply(base: &[Vertex], pos: &[Vec<[f32; 3]>], nor: &[Vec<[f32; 3]>], weights: &[f32], out: &mut Vec<Vertex>) {
    out.clear();
    out.extend_from_slice(base);
    for (k, &w) in weights.iter().enumerate() {
        if w == 0.0 { continue; }
        if let Some(d) = pos.get(k) { for (v, d) in out.iter_mut().zip(d) { for c in 0..3 { v.pos[c] += d[c] * w; } } }
        if let Some(d) = nor.get(k) { for (v, d) in out.iter_mut().zip(d) { for c in 0..3 { v.normal[c] += d[c] * w; } } }
    }
}

impl Kit {
    /// 形にモーフの差分を持たせる（glTF の targets の POSITION・NORMAL。頂点の数は形と同じ）。glb::instantiate_skinned は
    /// モーフのある部分に自分で呼ぶ。手で骨付きの形を作るときに使う
    pub fn set_geometry_morphs(&mut self, geo: GeoId, base: &crate::geometry::Geometry, pos: &[Vec<[f32; 3]>], nor: &[Vec<[f32; 3]>]) {
        let (verts, _) = super::vertices_of(base);
        self.morph_src.insert(geo.0, MorphSrc { base: verts, pos: pos.to_vec(), nor: nor.to_vec() });
    }
    pub(crate) fn has_geometry_morphs(&self, geo: GeoId) -> bool { self.morph_src.contains_key(&geo.0) }

    /// 骨付きの節点（scene.skinned。glb::instantiate_skinned の SkinnedModel.meshes）のモーフの重み（three の mesh.morphTargetInfluences）。
    /// 番号は glTF の targets の順（名前は Glb の meshes[..].target_names）。毎コマ呼んでよい（変わったコマだけ頂点を作り直して送る）。
    /// 既定は全部 0（three の GLTFLoader が入れる mesh.weights の初めの値は入れない。要るならゲームが渡す）
    pub fn set_morph_weights(&mut self, node: NodeId, weights: &[f32]) {
        let uid = self.scene.uid(node);
        let e = self.morph_nodes.entry(node.0).or_insert_with(|| MorphNode { uid, weights: Vec::new(), dirty: true, buf: None });
        if e.uid != uid { *e = MorphNode { uid, weights: Vec::new(), dirty: true, buf: None }; }
        if e.weights.as_slice() != weights { e.weights.clear(); e.weights.extend_from_slice(weights); e.dirty = true; }
    }
    /// 今の重み（set_morph_weights で渡した物。無ければ空）
    pub fn morph_weights(&self, node: NodeId) -> &[f32] {
        self.morph_nodes.get(&node.0).filter(|e| e.uid == self.scene.uid(node)).map_or(&[], |e| e.weights.as_slice())
    }

    /// 描く前に、重みが変わった節点の頂点を作って送る（1 コマに何度呼んでもよい）
    pub(crate) fn upload_morph(&mut self, node: u32, geo: u32) {
        let uid = self.scene.nodes()[node as usize].uid;
        let Some(e) = self.morph_nodes.get_mut(&node) else { return };
        if e.uid != uid { self.morph_nodes.remove(&node); return; }
        if !e.dirty { return; }
        e.dirty = false;
        let Some(src) = self.morph_src.get(&geo) else { return };
        if e.weights.iter().all(|w| *w == 0.0) { return; }
        let mut out = std::mem::take(&mut self.morph_scratch);
        apply(&src.base, &src.pos, &src.nor, &e.weights, &mut out);
        let bytes: &[u8] = bytemuck::cast_slice(&out);
        if e.buf.as_ref().is_none_or(|b| b.size() < bytes.len() as u64) {
            if let Some(b) = e.buf.take() { b.destroy(); }
            e.buf = Some(self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("morph vb"), size: bytes.len() as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        self.queue.write_buffer(e.buf.as_ref().unwrap(), 0, bytes);
        self.morph_scratch = out;
    }

    /// この節点が読む変えた頂点の列（重みが全部 0・モーフの無い形なら None で、形の頂点のまま）
    pub(crate) fn morph_vb(&self, node: u32, geo_id: u32) -> Option<&wgpu::Buffer> {
        if !self.morph_src.contains_key(&geo_id) { return None; }
        let e = self.morph_nodes.get(&node)?;
        if e.uid != self.scene.nodes()[node as usize].uid || e.weights.iter().all(|w| *w == 0.0) { return None; }
        e.buf.as_ref()
    }

    pub(crate) fn destroy_morphs(&mut self) {
        for e in self.morph_nodes.values_mut() { if let Some(b) = e.buf.take() { b.destroy(); } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 防ぐ故障: まばたきの重みで形が動かない・重み 0 の差分まで足して形が崩れる・法線だけ動かない
    #[test]
    fn morph_adds_weighted_deltas_in_order() {
        let base = vec![Vertex { pos: [1.0, 2.0, 3.0], normal: [0.0, 0.0, 1.0], ..Default::default() }; 2];
        let pos = vec![vec![[1.0, 0.0, 0.0]; 2], vec![[0.0, 10.0, 0.0]; 2]];
        let nor = vec![vec![[0.0, 1.0, 0.0]; 2], vec![[0.0, 0.0, 0.0]; 2]];
        let mut out = Vec::new();
        apply(&base, &pos, &nor, &[0.5, 0.0], &mut out);
        assert_eq!(out[1].pos, [1.5, 2.0, 3.0]);
        assert_eq!(out[1].normal, [0.0, 0.5, 1.0]);
        apply(&base, &pos, &nor, &[0.25, 0.1], &mut out);
        assert_eq!(out[0].pos, [1.0 + 1.0 * 0.25, 2.0 + 10.0 * 0.1, 3.0]);
        apply(&base, &pos, &nor, &[], &mut out);
        assert_eq!(out[0].pos, base[0].pos);
    }
}
