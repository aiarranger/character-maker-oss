// 主役を Kit から Kit へ持ち回る口（kit.take_actor・put_actor）の確かめ（Mac の Metal で Kit::headless を描いて読み戻す）。
// 防ぐ故障: 場所を移って Kit を作り直すとき、持ち回った主役が描けない・前の Kit の release_gpu で持ち回った主役の画像まで消える・
// runtime_template で作り直した動く部品で、組み立てた直後と違う絵になる。
// 主役は aisns の素の .glb（クレオパトラ）を主役の部品で読む。aisns のフォルダが無ければ飛ばす。
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
    // 別の物を先に作って、2 つの Kit で節点の番号をずらす
    let _ = kit.scene.group(None);
    kit.scene.actor(None)
}

#[test]
fn carried_actor_draws_the_same_in_the_next_kit() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../aisns/public/models/welcome/cleopatra-0529d46fe4fe.glb");
    let Ok(bytes) = std::fs::read(&path) else { println!("aisns の模型が無いので飛ばす"); return };
    let gpu = block_on(Gpu::new()).expect("GPU が無い");
    let file = character_actor_npc::package::read_character_file(&bytes, "cleopatra.glb").unwrap();
    let asset = character_actor::package::CharacterAsset { source: character_actor::package::AssetSource::Glb, manifest: file.manifest.clone(), model: Arc::new(file.model.to_vec()), thumbnail: None, stats: None };
    let step = |kit: &Kit, node, a: &mut character_actor::actor::CharacterActor| {
        a.move_runtime(false, false);
        a.set_parent_world(kit.actor_world(node));
        a.update(1.0 / 30.0, 0.0);
    };
    // 1 つ目の Kit: 組み立てた直後の主役
    let mut a = Kit::headless(&gpu, 96, 96);
    let node_a = a.scene.actor(None);
    a.camera.position = Vec3::new(0.0, 0.8, 2.6);
    a.camera.look_at(Vec3::new(0.0, 0.6, 0.0));
    let mut h = character_actor::package::assemble_package(&asset, None).unwrap();
    let template = h.runtime_template().unwrap();
    step(&a, node_a, &mut h);
    a.set_actor(node_a, h);
    a.render(&RenderOpts::default());
    let first = read(&gpu, a.headless_output().unwrap());
    let bg = first[..4].to_vec();
    let covered = first.chunks_exact(4).filter(|p| *p != bg.as_slice()).count();
    assert!(covered > 300, "主役が映っていない {covered}");
    // 取り出して、1 つ目の Kit を片付ける
    let mut carried = a.take_actor(node_a).expect("取り出せない");
    assert!(!a.has_actor(node_a));
    a.release_gpu();
    // 2 つ目の Kit（同じ Gpu）へ。動く部品は runtime_template で作り直す（組み立てた直後と同じ状態）
    let mut b = Kit::headless(&gpu, 96, 96);
    let node_b = scene(&mut b);
    let mut fresh = template.spawn();
    step(&b, node_b, &mut fresh);
    carried.actor = Some(fresh);
    b.put_actor(node_b, carried);
    assert!(b.has_actor(node_b));
    b.render(&RenderOpts::default());
    let second = read(&gpu, b.headless_output().unwrap());
    let differ = first.chunks_exact(4).zip(second.chunks_exact(4)).filter(|(x, y)| x != y).count();
    assert_eq!(differ, 0, "持ち回った主役が {differ} 画素違う");
    // 片付けたあとは描かない（入れ物を消すと GPU の物を返す）
    b.scene.remove(node_b);
    b.render(&RenderOpts::default());
    let gone = read(&gpu, b.headless_output().unwrap());
    assert!(gone.chunks_exact(4).all(|p| p == bg.as_slice()), "消した主役が残っている");
    println!("持ち回った主役の画素 {covered}・同じ絵");
}
