// NPC・汎用キャラ（character-actor-npc の NpcActor・character-view-npc の NpcView）を描く口。主役の kit.set_actor と同じ形で、別の部品で描く
// （Owner 2026-11-12「種類ごとに分ける」）。入れ物は主役と同じ kit.scene.actor の節点。1 つの節点には主役か NPC のどちらか 1 つ（後から入れた方）。
use super::{DEPTH, HDR, Kit, MASK};
use crate::{Color, NodeId};
use character_actor_npc::actor::NpcActor;
use character_view::{ActorLook, ActorStyle};
use character_view_npc::{NpcLook, NpcPipelines, NpcStyle, NpcTargets, NpcView};
use glam::Mat4;
use std::sync::Arc;

pub(crate) struct NpcSlot {
    pub uid: u32,
    pub view: NpcView,
    pub actor: Option<NpcActor>,
    pub look: ActorLook,
}

pub(crate) fn npc_look(l: &ActorLook) -> NpcLook {
    NpcLook { rim: l.rim, rim_strength: l.rim_strength, aura: l.aura, aura_strength: l.aura_strength, top: l.top, lift: l.lift, flash: l.flash, hidden: l.hidden, hidden_opacity: l.hidden_opacity }
}
fn npc_style(s: ActorStyle) -> NpcStyle { NpcStyle { flat_lift: s.flat_lift, rim_pow: s.rim_pow, sat: s.sat, top_self: s.top_self } }

impl Kit {
    fn npc_pipelines(&mut self) -> Arc<NpcPipelines> {
        if let Some(p) = &self.npc_pipes { return p.clone(); }
        let p = Arc::new(NpcPipelines::new(&self.device, &self.queue, &self.globals_layout, NpcTargets { color: HDR, depth: DEPTH, mask: MASK, samples: self.samples }));
        self.npc_pipes = Some(p.clone());
        p
    }

    /// NPC・汎用を入れる（GPU へ上げる。上げた形は actor から消える）。kit.set_gpu_mips(true) のあとなら画像のミップを GPU で作る。
    /// 毎コマ: actor（npc_actor_mut）に set_parent_world(kit.actor_world(node))・move_runtime などを渡して update
    pub fn set_npc_actor(&mut self, node: NodeId, mut actor: NpcActor) {
        let pipes = self.npc_pipelines();
        let look = self.npc_actors.get(&node).map_or_else(|| self.actors.get(&node).map_or_else(|| self.default_look(), |s| s.look), |s| s.look);
        let mut view = NpcView::with_pipelines(&self.device, pipes);
        view.set_style(npc_style(self.actor_style));
        view.set_gpu_mips(self.gpu_mips);
        view.set_actor(&self.device, &self.queue, &mut actor);
        self.clear_actor(node);
        self.npc_actors.insert(node, NpcSlot { uid: self.scene.uid(node), view, actor: Some(actor), look });
    }
    /// 同じ人をもう 1 体（from の GPU の形を共有する。actor は同じ人を読み直した物）
    pub fn share_npc_actor(&mut self, from: NodeId, node: NodeId, mut actor: NpcActor) {
        let look = self.npc_actors.get(&node).map_or_else(|| self.default_look(), |s| s.look);
        let view = NpcView::share_from(&self.npc_actors[&from].view, &self.device);
        NpcView::release_shapes(&mut actor);
        self.clear_actor(node);
        self.npc_actors.insert(node, NpcSlot { uid: self.scene.uid(node), view, actor: Some(actor), look });
    }
    /// NPC・汎用を外す（入れ物は残る）。GPU の物を返す（同じ形を使う体が残っていれば、形と画像はその体が持つ）
    pub fn clear_npc_actor(&mut self, node: NodeId) { if let Some(mut s) = self.npc_actors.remove(&node) { s.view.destroy_gpu(); } }
    pub fn has_npc_actor(&self, node: NodeId) -> bool { self.npc_actors.get(&node).is_some_and(|s| s.actor.is_some()) }
    /// 動かす部品（入っていなければ止まる。has_npc_actor で確かめる）
    pub fn npc_actor_mut(&mut self, node: NodeId) -> &mut NpcActor { self.npc_actors.get_mut(&node).and_then(|s| s.actor.as_mut()).expect("NPC が入っていない") }
    pub fn npc_actor(&self, node: NodeId) -> &NpcActor { self.npc_actors.get(&node).and_then(|s| s.actor.as_ref()).expect("NPC が入っていない") }
    /// 見せ方の値（主役と同じ型。物陰の影絵の色は kit.hidden_color）
    pub fn set_npc_actor_look(&mut self, node: NodeId, look: ActorLook) { if let Some(s) = self.npc_actors.get_mut(&node) { s.look = look; } }
    /// 体の濃さ（0..1。1 未満は半透明の描き方。NpcView::set_opacity）
    pub fn set_npc_actor_opacity(&mut self, node: NodeId, k: f32) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_opacity(k); } }
    pub fn set_npc_actor_fade(&mut self, node: NodeId, k: f32) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_fade(k); } }
    pub fn set_npc_actor_tint(&mut self, node: NodeId, r: f32, g: f32, b: f32) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_tint([r, g, b]); } }
    pub fn set_npc_actor_sun_rim(&mut self, node: NodeId, color: Color, k: f32) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_sun_rim([color.r, color.g, color.b], k); } }
    /// 武器の模型（character-actor-npc の glb で読んだ物。持ち手は actor.set_grip_world）
    pub fn set_npc_actor_weapon(&mut self, node: NodeId, g: &character_actor_npc::glb::Glb) {
        if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_weapon(&self.device, &self.queue, g); }
    }
    pub fn clear_npc_actor_weapon(&mut self, node: NodeId) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.clear_weapon(); } }
    pub fn set_npc_actor_weapon_unlit(&mut self, node: NodeId, on: bool) { if let Some(s) = self.npc_actors.get_mut(&node) { s.view.set_weapon_unlit(on); } }
    /// 入れ物の世界の行列（kit.actor_world と同じ）
    pub fn npc_actor_world(&self, node: NodeId) -> Mat4 { self.scene.world_matrix(node) }
}
