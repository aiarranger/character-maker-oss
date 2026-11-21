//! Opt-in opaque-body batching for views made by ActorView::share_from.
//! The existing per-actor writer and transparent/HurtFace draws use slices of
//! these same pose buffers. No duplicate pose upload or GPU completion wait.
use super::*;
use std::ops::Range;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
struct InstanceSlot {
    palette_base: u32,
    morph_base: u32,
    hero_base: u32,
    actor_id: u32,
}

#[derive(Clone, Copy)]
struct PoseLayout {
    palette_stride: u64,
    morph_stride: u64,
    hero_stride: u64,
}
impl PoseLayout {
    fn new(palette_size: u64, morph_size: u64, storage_align: u32, uniform_align: u32) -> Self {
        let align = |n: u64, a: u64| n.div_ceil(a) * a;
        Self {
            palette_stride: align(palette_size, u64::from(storage_align).max(64)),
            morph_stride: align(morph_size, u64::from(storage_align).max(4)),
            hero_stride: align(
                std::mem::size_of::<HeroU>() as u64,
                u64::from(uniform_align).max(16),
            ),
        }
    }
    fn slot(self, id: usize) -> InstanceSlot {
        InstanceSlot {
            palette_base: u32::try_from(id as u64 * self.palette_stride / 64)
                .expect("palette offset"),
            morph_base: u32::try_from(id as u64 * self.morph_stride / 4).expect("morph offset"),
            hero_base: u32::try_from(id as u64 * self.hero_stride / 16).expect("hero offset"),
            actor_id: u32::try_from(id).expect("actor ID"),
        }
    }
}

/// Fixed-capacity, optional renderer for the opaque primitives of one shared
/// shape. After construction, update these views only with write_frame; replacing
/// their actor/weapon invalidates the batch. Create a new batch at setup changes.
/// Transparent/HurtFace drawing remains on the original ActorView API/order.
pub struct ActorOpaqueBatch {
    opaque: [wgpu::RenderPipeline; 2],
    empty_bg: wgpu::BindGroup,
    data_bg: wgpu::BindGroup,
    vb: wgpu::Buffer,
    skin: wgpu::Buffer,
    ib: wgpu::Buffer,
    tex: Vec<wgpu::BindGroup>,
    items: Vec<DrawItem>,
    slots: Vec<InstanceSlot>,
    instance_buffer: wgpu::Buffer,
    instances: Vec<InstanceSlot>,
    instance_ids: Vec<u32>,
    ranges: Vec<Range<u32>>,
    upload_bytes: u64,
}

// A primitive-major list retains stable actor IDs when an intermediate enemy
// dies or a particular primitive is hidden. Transparent items never enter it.
fn collect_instances(
    items: &[DrawItem],
    active_ids: &[u32],
    slots: &[InstanceSlot],
    visible: impl Fn(usize, &DrawItem) -> bool,
    instances: &mut Vec<InstanceSlot>,
    ids: &mut Vec<u32>,
    ranges: &mut [Range<u32>],
) {
    instances.clear();
    ids.clear();
    for (item, range) in items.iter().zip(ranges) {
        let start = instances.len() as u32;
        if !item.transparent {
            for &id in active_ids {
                if visible(id as usize, item) {
                    instances.push(slots[id as usize]);
                    ids.push(id);
                }
            }
        }
        *range = start..instances.len() as u32;
    }
}

impl ActorOpaqueBatch {
    /// All views must be unarmed share_from instances of the same shape.
    /// Allocation, pipeline creation and re-binding happen only here, not per frame.
    pub fn new(
        device: &wgpu::Device,
        globals: &wgpu::BindGroupLayout,
        targets: ActorTargets,
        views: &mut [ActorView],
    ) -> Self {
        assert!(!views.is_empty());
        let first = &views[0];
        let source = first.gpu.as_ref().expect("uploaded actor shape");
        for view in views.iter() {
            let gpu = view.gpu.as_ref().expect("uploaded actor shape");
            assert!(
                Arc::ptr_eq(&gpu.draw_init, &source.draw_init),
                "batch requires shared mesh/materials"
            );
            assert!(
                Arc::ptr_eq(&view.pipes, &first.pipes),
                "batch requires shared pipelines"
            );
            assert!(view.weapon.is_none(), "opaque batch is unarmed body only");
        }
        let items: Vec<_> = source
            .items
            .iter()
            .filter(|it| !it.transparent)
            .cloned()
            .collect();
        let (vb, skin, ib, tex) = (
            source.vb.clone(),
            source.skin.clone(),
            source.ib.clone(),
            source.tex.clone(),
        );
        let (palette_size, morph_size) = (source.palette_size, source.weights_size);
        let limits = device.limits();
        let layout = PoseLayout::new(
            palette_size,
            morph_size,
            limits.min_storage_buffer_offset_alignment,
            limits.min_uniform_buffer_offset_alignment,
        );
        let count = views.len();
        let make_buffer = |label, size, extra| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | extra,
                mapped_at_creation: false,
            })
        };
        let palettes = make_buffer(
            "batch palettes",
            layout.palette_stride * count as u64,
            wgpu::BufferUsages::empty(),
        );
        let weights = make_buffer(
            "batch morphs",
            layout.morph_stride * count as u64,
            wgpu::BufferUsages::empty(),
        );
        let heroes = make_buffer(
            "batch looks",
            layout.hero_stride * count as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let (v, vf) = (
            wgpu::ShaderStages::VERTEX,
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let ro = wgpu::BufferBindingType::Storage { read_only: true };
        let data_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opaque batch data"),
            entries: &[
                slot_entry(0, v, ro, false),
                slot_entry(1, v, ro, false),
                slot_entry(2, v, ro, false),
                slot_entry(3, vf, ro, false),
                slot_entry(4, vf, wgpu::BufferBindingType::Uniform, true),
            ],
        });
        let data_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("opaque batch"),
            layout: &data_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: palettes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source.deltas.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: weights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: heroes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: buffer_range((
                        &source.draw_u,
                        0,
                        std::mem::size_of::<DrawU>() as u64,
                    )),
                },
            ],
        });
        let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("batch empty"),
            entries: &[],
        });
        let empty_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &empty,
            entries: &[],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("opaque batch"),
            bind_group_layouts: &[
                Some(globals),
                Some(&empty),
                Some(&first.pipes.tex_layout),
                Some(&data_layout),
            ],
            immediate_size: 0,
        });
        let shader = shader(
            device,
            "opaque batch",
            &hero_source(include_str!("../shaders/hero_instanced.wgsl")),
        );
        const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![7 => Uint32x4];
        let opaque = [false, true].map(|double| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("opaque body instances"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(vert_layout()),
                        Some(skin_layout()),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 16,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &ATTRS,
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_basic"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: targets.color,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: if double { None } else { Some(wgpu::Face::Back) },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: targets.depth,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: targets.samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        });
        let max_instances = items.len() * count;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opaque actor slots"),
            size: (max_instances.max(1) * std::mem::size_of::<InstanceSlot>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for (id, view) in views.iter_mut().enumerate() {
            let hg = view.gpu.as_mut().unwrap();
            hg.palette = palettes.clone();
            hg.palette_offset = id as u64 * layout.palette_stride;
            hg.weights = weights.clone();
            hg.weights_offset = id as u64 * layout.morph_stride;
            view.hero_u = heroes.clone();
            view.hero_offset = id as u64 * layout.hero_stride;
            hg.data_bg = data_bind(
                device,
                &view.pipes.data_layout,
                (&hg.palette, hg.palette_offset, hg.palette_size),
                &hg.deltas,
                (&hg.weights, hg.weights_offset, hg.weights_size),
                (&view.hero_u, view.hero_offset),
                &hg.draw_u,
            );
        }
        Self {
            opaque,
            empty_bg,
            data_bg,
            vb,
            skin,
            ib,
            tex,
            ranges: vec![0..0; items.len()],
            items,
            slots: (0..count).map(|id| layout.slot(id)).collect(),
            instance_buffer,
            instances: Vec::with_capacity(max_instances),
            instance_ids: Vec::with_capacity(max_instances),
            upload_bytes: 0,
        }
    }

    /// active_ids use the original actor slice index, including death-flash actors.
    /// Caller writes each matching ActorView once before encoding either path.
    pub fn prepare(&mut self, queue: &wgpu::Queue, actors: &[CharacterActor], active_ids: &[u32]) {
        assert_eq!(actors.len(), self.slots.len());
        assert!(active_ids.len() <= self.slots.len());
        collect_instances(
            &self.items,
            active_ids,
            &self.slots,
            |id, item| ActorView::visible(&actors[id], item),
            &mut self.instances,
            &mut self.instance_ids,
            &mut self.ranges,
        );
        let bytes = bytemuck::cast_slice(&self.instances);
        self.upload_bytes = bytes.len() as u64;
        if !bytes.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytes);
        }
    }

    /// Returns actual draw calls and the sum of instances submitted across body
    /// primitives (not a unique character count). group 0 is set by the caller.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> (u32, u32) {
        let mut calls = 0;
        let mut instances = 0;
        pass.set_bind_group(1, &self.empty_bg, &[]);
        pass.set_vertex_buffer(0, self.vb.slice(..));
        pass.set_vertex_buffer(1, self.skin.slice(..));
        pass.set_vertex_buffer(2, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
        for (item, range) in self.items.iter().zip(&self.ranges) {
            if range.is_empty() {
                continue;
            }
            pass.set_pipeline(&self.opaque[item.double as usize]);
            pass.set_bind_group(2, &self.tex[item.tex], &[]);
            pass.set_bind_group(3, &self.data_bg, &[item.offset]);
            pass.draw_indexed(
                item.istart..item.istart + item.icount,
                item.base_vertex,
                range.clone(),
            );
            calls += 1;
            instances += range.end - range.start;
        }
        (calls, instances)
    }
    pub fn instance_upload_bytes(&self) -> u64 {
        self.upload_bytes
    }
    /// Primitive-major stable actor IDs, for snapshot-time diagnosis only.
    pub fn instance_ids(&self) -> &[u32] {
        &self.instance_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(prim: usize, transparent: bool) -> DrawItem {
        DrawItem {
            prim: Some(prim),
            istart: 0,
            icount: 3,
            base_vertex: 0,
            offset: 0,
            tex: 0,
            double: false,
            transparent,
            alpha_shadow: false,
            base_color: [1.0; 4],
            base_p: [0.0; 4],
        }
    }

    #[test]
    fn compaction_preserves_actor_slots_and_per_primitive_visibility() {
        let layout = PoseLayout::new(320, 20, 256, 256);
        let slots: Vec<_> = (0..5).map(|id| layout.slot(id)).collect();
        let items = [item(0, false), item(1, true), item(2, false)];
        // IDs 1 and 3 are dead without flash; 2 is dead but still flashing.
        let alive = [true, false, false, false, true];
        let flash = [0.0, 0.0, 0.1, 0.0, 0.0];
        let active: Vec<u32> = (0..5)
            .filter(|&id| alive[id] || flash[id] > 0.0)
            .map(|id| id as u32)
            .collect();
        let mut instances = Vec::with_capacity(15);
        let mut ids = Vec::with_capacity(15);
        let mut ranges = vec![0..0; items.len()];
        collect_instances(
            &items,
            &active,
            &slots,
            |id, item| !(id == 2 && item.prim == Some(0)),
            &mut instances,
            &mut ids,
            &mut ranges,
        );
        assert_eq!(ids, [0, 4, 0, 2, 4]);
        assert_eq!(ranges, [0..2, 2..2, 2..5]);
        for (&id, slot) in ids.iter().zip(&instances) {
            assert_eq!(*slot, slots[id as usize]);
            assert_eq!(slot.palette_base, id * 8); // 320-byte palette padded to 512.
            assert_eq!(slot.morph_base, id * 64);
            assert_eq!(slot.hero_base, id * 16);
        }
        let caps = (instances.capacity(), ids.capacity());
        // Expiring the middle death flash must never shift ID 4's pose/flash slot.
        collect_instances(
            &items,
            &[0, 4],
            &slots,
            |_, _| true,
            &mut instances,
            &mut ids,
            &mut ranges,
        );
        assert_eq!(ids, [0, 4, 0, 4]);
        assert_eq!(instances[1], slots[4]);
        assert_eq!(caps, (instances.capacity(), ids.capacity()));
        collect_instances(
            &items,
            &[],
            &slots,
            |_, _| true,
            &mut instances,
            &mut ids,
            &mut ranges,
        );
        assert!(instances.is_empty());
        assert!(ranges.iter().all(Range::is_empty));
    }

    #[test]
    fn shared_pose_ranges_and_shader_struct_offsets_match() {
        assert_eq!(std::mem::size_of::<HeroU>(), 9 * 16);
        assert_eq!(std::mem::offset_of!(HeroU, misc), 2 * 16);
        assert_eq!(std::mem::offset_of!(HeroU, nfix), 5 * 16);
        assert_eq!(std::mem::offset_of!(HeroU, sun), 8 * 16);
        let layout = PoseLayout::new(320, 20, 256, 256);
        let mut bytes = vec![0u8; 5 * layout.hero_stride as usize];
        for id in 0..5 {
            let mut hero = HeroU::zeroed();
            hero.misc[2] = id as f32 * 0.1;
            hero.sun = [id as f32, 0.42, 0.38, -1.0];
            let at = id * layout.hero_stride as usize;
            bytes[at..at + std::mem::size_of::<HeroU>()].copy_from_slice(bytemuck::bytes_of(&hero));
        }
        for id in [0, 2, 4] {
            let slot = layout.slot(id);
            let at = slot.hero_base as usize * 16;
            let hero: HeroU =
                bytemuck::pod_read_unaligned(&bytes[at..at + std::mem::size_of::<HeroU>()]);
            assert_eq!(hero.misc[2], id as f32 * 0.1);
            assert_eq!(hero.sun, [id as f32, 0.42, 0.38, -1.0]);
        }
    }

    #[test]
    fn legacy_and_instanced_wgsl_validate_with_shared_math() {
        for entry in [
            include_str!("../shaders/hero.wgsl"),
            include_str!("../shaders/hero_instanced.wgsl"),
        ] {
            let src = format!("{}\n{}", crate::COMMON, hero_source(entry));
            let module = wgpu::naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|err| panic!("{}", err.emit_to_string(&src)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|err| panic!("{}", err.emit_to_string(&src)));
        }
    }
}
