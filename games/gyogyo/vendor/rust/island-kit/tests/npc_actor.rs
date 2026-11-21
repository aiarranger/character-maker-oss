// NPC・汎用の描く口（kit.set_npc_actor。character-actor-npc・character-view-npc）の確かめ（Mac の Metal で Kit::headless を描いて読み戻す）。
// 防ぐ故障: NPC を別の部品へ分けたとき、kit で描いた NPC の絵が、分ける前の描き方（主役の部品 kit.set_actor）と変わる。入れ物を消すと描かなくなる。
// 比べる NPC は aisns の素の .glb（クレオパトラ）。aisns のフォルダが無ければ飛ばす。
use glam::Vec3;
use island_kit::{Gpu, Kit, RenderOpts};
use std::path::PathBuf;
use std::sync::Arc;

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; }
        std::thread::yield_now();
    }
}

fn read(gpu: &Gpu, tex: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (tex.width(), tex.height());
    let row = (w * 4).div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(Some(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    (0..h).flat_map(|y| data[(y * row) as usize..(y * row + w * 4) as usize].to_vec()).collect()
}

fn scene(kit: &mut Kit) -> island_kit::NodeId {
    kit.camera.position = Vec3::new(0.0, 0.8, 2.6);
    kit.camera.look_at(Vec3::new(0.0, 0.6, 0.0));
    kit.scene.actor(None)
}

#[test]
fn npc_actor_draws_like_before_the_split() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../aisns/public/models/welcome/cleopatra-0529d46fe4fe.glb");
    let Ok(bytes) = std::fs::read(&path) else { println!("aisns の NPC の模型が無いので飛ばす"); return };
    let gpu = block_on(Gpu::new()).expect("GPU が無い");
    let npc = character_actor_npc::package::read_character_file(&bytes, "cleopatra.glb").unwrap();
    // NPC の部品で描く
    let mut a = Kit::headless(&gpu, 96, 96);
    let node = scene(&mut a);
    let mut actor = character_actor_npc::package::assemble_package(&npc).unwrap();
    actor.set_npc(true);
    actor.set_parent_world(a.npc_actor_world(node));
    actor.update(1.0 / 30.0, 0.0);
    a.set_npc_actor(node, actor);
    a.render(&RenderOpts::default());
    let by_npc = read(&gpu, a.headless_output().unwrap());
    // 分ける前の描き方: 同じ manifest と GLB を主役の部品で（NPC の GLB は光の色だけの材質を持たないので、0.2.5 と 0.2.7 の材質の違いは出ない）
    let mut b = Kit::headless(&gpu, 96, 96);
    let node_b = scene(&mut b);
    let hero = character_actor::package::CharacterAsset { source: character_actor::package::AssetSource::Glb, manifest: npc.manifest.clone(), model: Arc::new(npc.model.to_vec()), thumbnail: None, stats: None };
    let mut h = character_actor::package::assemble_package(&hero, None).unwrap();
    h.set_npc(true);
    h.move_runtime(false, false);
    h.set_parent_world(b.actor_world(node_b));
    h.update(1.0 / 30.0, 0.0);
    b.set_actor(node_b, h);
    b.render(&RenderOpts::default());
    let by_hero = read(&gpu, b.headless_output().unwrap());
    let bg = by_npc[..4].to_vec();
    let covered = by_npc.chunks_exact(4).filter(|p| *p != bg.as_slice()).count();
    assert!(covered > 300, "NPC が映っていない {covered}");
    let differ = by_npc.chunks_exact(4).zip(by_hero.chunks_exact(4)).filter(|(x, y)| x != y).count();
    assert_eq!(differ, 0, "分ける前の描き方と {differ} 画素違う");
    // 入れ物を消すと描かない
    a.scene.remove(node);
    a.render(&RenderOpts::default());
    let gone = read(&gpu, a.headless_output().unwrap());
    assert!(gone.chunks_exact(4).all(|p| p == bg.as_slice()), "消した NPC が残っている");
    println!("NPC の部品で描いた画素 {covered}・分ける前と同じ");
}
