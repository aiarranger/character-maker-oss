// three の BatchedMesh の描き方。毎コマ・視野ごと（主のカメラ・影の光）に、見えるインスタンスを形ごとに集めて、1 コマのインスタンスの流れへ入れる。
// 形ごとの InstancedMesh を毎コマ入れ直す形にしないのは、1 本の行列の列では主と影で別の間引きができないから（遠くまで見える林は主の視野にあるが、影の範囲は ±28〜40 m）。
use super::frame::{Frustum, InstRaw, max_scale};
use crate::scene::Batch;
use bytemuck::Zeroable;
use glam::{Mat4, Vec3};

/// 同じ形を続けて描く 1 まとまり（流れの first から count 個）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Run { pub shape: u32, pub first: u32, pub count: u32 }

/// 毎コマの作業の入れ物（割り当てを減らす）
#[derive(Default)]
pub(crate) struct Scratch { order: Vec<u32>, counts: Vec<u32>, at: Vec<u32> }

/// bounds は形ごとの包む球（three の getBoundingSphereAt）。fx_w は流れの fx.w: None なら色の 4 つ目（Custom の材質が 4 つの値を読む）、Some なら receiveShadow（組み込みの材質）
pub(crate) fn fill(b: &Batch, world: Mat4, planes: &Frustum, bounds: &[(Vec3, f32)], fx_w: Option<f32>, stream: &mut Vec<InstRaw>, runs: &mut Vec<Run>, s: &mut Scratch) {
    s.counts.clear();
    s.counts.resize(b.shapes.len(), 0);
    s.order.clear();
    for (k, it) in b.insts.iter().enumerate() {
        if !it.visible { continue; }
        if b.per_object_culled {
            // three の Sphere.applyMatrix4 と Frustum.intersectsSphere
            let m = world * it.matrix;
            let (c, r) = bounds[it.shape as usize];
            if !planes.intersects(m.transform_point3(c), r * max_scale(m)) { continue; }
        }
        s.counts[it.shape as usize] += 1;
        s.order.push(k as u32);
    }
    let base = stream.len() as u32;
    let mut first = base;
    s.at.clear();
    for (shape, c) in s.counts.iter().enumerate() {
        s.at.push(first);
        if *c > 0 { runs.push(Run { shape: shape as u32, first, count: *c }); first += *c; }
    }
    stream.resize(first as usize, InstRaw::zeroed());
    for k in &s.order {
        let it = &b.insts[*k as usize];
        let at = &mut s.at[it.shape as usize];
        let c = it.color;
        stream[*at as usize] = InstRaw { m: (world * it.matrix).to_cols_array(), fx: [c[0], c[1], c[2], fx_w.unwrap_or(c[3])] };
        *at += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{GeoId, MatId};
    use crate::scene::{Camera, NodeKind, Scene};

    fn eye() -> Frustum {
        let mut c = Camera::new(60.0, 1.0, 0.5, 100.0);
        c.position = Vec3::new(0.0, 0.0, 10.0);
        c.look_at(Vec3::ZERO);
        Frustum::new(c.view_proj())
    }
    fn batch(s: &mut Scene, shapes: usize) -> crate::scene::NodeId {
        let id = s.batched(None, MatId(0));
        for k in 0..shapes { s.batch_add_shape(id, GeoId(k as u32)); }
        id
    }
    fn run(s: &Scene, id: crate::scene::NodeId, f: &Frustum, bounds: &[(Vec3, f32)], stream: &mut Vec<InstRaw>) -> Vec<Run> {
        let NodeKind::Batched(b) = &s[id].kind else { panic!() };
        let (mut runs, mut sc) = (Vec::new(), Scratch::default());
        fill(b, Mat4::IDENTITY, f, bounds, None, stream, &mut runs, &mut sc);
        runs
    }

    /// 防ぐ故障: 隠したインスタンスや画面の外の物を描く／setGeometryIdAt（遠近の切り替え）が効かない／別の形の位置で描く
    #[test]
    fn groups_by_shape_and_drops_hidden_and_offscreen() {
        let mut s = Scene::new();
        let id = batch(&mut s, 2);
        let bounds = [(Vec3::ZERO, 1.0), (Vec3::ZERO, 1.0)];
        let place = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
        let a = s.batch_add_instance(id, 0); s.set_batch_matrix(id, a, place(1.0));
        let b = s.batch_add_instance(id, 1); s.set_batch_matrix(id, b, place(2.0));
        let hidden = s.batch_add_instance(id, 0); s.set_batch_matrix(id, hidden, place(3.0)); s.set_batch_visible(id, hidden, false);
        let behind = s.batch_add_instance(id, 1); s.set_batch_matrix(id, behind, Mat4::from_translation(Vec3::new(0.0, 0.0, 30.0)));
        let c = s.batch_add_instance(id, 0); s.set_batch_matrix(id, c, place(-1.0));
        let mut stream = Vec::new();
        let runs = run(&s, id, &eye(), &bounds, &mut stream);
        assert_eq!(runs, vec![Run { shape: 0, first: 0, count: 2 }, Run { shape: 1, first: 2, count: 1 }]);
        assert_eq!(stream.iter().map(|i| i.m[12]).collect::<Vec<_>>(), vec![1.0, -1.0, 2.0]);
        // 遠近の切り替え: 同じインスタンスが別の形の列へ移る
        s.set_batch_shape(id, a, 1);
        stream.clear();
        let runs = run(&s, id, &eye(), &bounds, &mut stream);
        assert_eq!(runs, vec![Run { shape: 0, first: 0, count: 1 }, Run { shape: 1, first: 1, count: 2 }]);
        assert_eq!(stream.iter().map(|i| i.m[12]).collect::<Vec<_>>(), vec![-1.0, 1.0, 2.0]);
    }

    /// 防ぐ故障: 大きな木や、原点からずれた形（幹の根が原点で、球の中心は樹冠の高さ）が、画面の端でまだ見えているのに消える
    #[test]
    fn cull_uses_the_instance_scale_and_the_shape_center() {
        let mut s = Scene::new();
        let id = batch(&mut s, 1);
        let i = s.batch_add_instance(id, 0);
        let f = eye();
        let bounds = [(Vec3::ZERO, 2.0)];
        // 大きさ 1: 球の中心は x = 40、半径 2 で画面の外
        s.set_batch_matrix(id, i, Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0)));
        assert!(run(&s, id, &f, &bounds, &mut Vec::new()).is_empty());
        // 大きさ 20: 半径 40 で画面にかかる
        s.set_batch_matrix(id, i, Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0)) * Mat4::from_scale(Vec3::splat(20.0)));
        assert_eq!(run(&s, id, &f, &bounds, &mut Vec::new()).len(), 1);
        // 球の中心は行列で動く: 根は画面の外でも、樹冠の中心が画面の中
        let bounds = [(Vec3::new(0.0, 30.0, 0.0), 2.0)];
        s.set_batch_matrix(id, i, Mat4::from_translation(Vec3::new(0.0, -30.0, 0.0)));
        assert_eq!(run(&s, id, &f, &bounds, &mut Vec::new()).len(), 1);
    }

    /// 防ぐ故障: 主の視野と影の光で同じ流れを使うときに、後の視野が前の視野の位置を上書きする／間引きを切っても（perObjectFrustumCulled = false）消える
    #[test]
    fn two_views_share_the_stream_and_culling_can_be_turned_off() {
        let mut s = Scene::new();
        let id = batch(&mut s, 1);
        let i = s.batch_add_instance(id, 0);
        s.set_batch_matrix(id, i, Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)));
        let bounds = [(Vec3::ZERO, 1.0)];
        let mut stream = Vec::new();
        let first = run(&s, id, &eye(), &bounds, &mut stream);
        let second = run(&s, id, &eye(), &bounds, &mut stream);
        assert_eq!(first, vec![Run { shape: 0, first: 0, count: 1 }]);
        assert_eq!(second, vec![Run { shape: 0, first: 1, count: 1 }]);
        assert_eq!(stream.len(), 2);
        s.set_batch_matrix(id, i, Mat4::from_translation(Vec3::new(500.0, 0.0, 0.0)));
        assert!(run(&s, id, &eye(), &bounds, &mut Vec::new()).is_empty());
        if let NodeKind::Batched(b) = &mut s[id].kind { b.per_object_culled = false; }
        assert_eq!(run(&s, id, &eye(), &bounds, &mut Vec::new()).len(), 1);
    }

    /// 防ぐ故障: 葉の色の 4 つの値の 4 つ目が receiveShadow で潰れる（Custom の材質）／組み込みの材質の影が受けられない
    #[test]
    fn color_fourth_value_goes_to_custom_only() {
        let mut s = Scene::new();
        let id = batch(&mut s, 1);
        let i = s.batch_add_instance(id, 0);
        s.set_batch_color(id, i, [0.9, 0.1, 0.2, 0.3]);
        let NodeKind::Batched(b) = &s[id].kind else { panic!() };
        let bounds = [(Vec3::ZERO, 1.0)];
        let (mut stream, mut runs, mut sc) = (Vec::new(), Vec::new(), Scratch::default());
        fill(b, Mat4::IDENTITY, &eye(), &bounds, None, &mut stream, &mut runs, &mut sc);
        assert_eq!(stream[0].fx, [0.9, 0.1, 0.2, 0.3]);
        stream.clear();
        fill(b, Mat4::IDENTITY, &eye(), &bounds, Some(1.0), &mut stream, &mut runs, &mut sc);
        assert_eq!(stream[0].fx, [0.9, 0.1, 0.2, 1.0]);
    }
}
