// 大将「黄肌大王」: 宙を泳ぐ巨大なキハダ（view/fish.rs の FishBoss）。ここは当たりの白い光・怒りの赤い光を決めて渡す入口。
use crate::gfx::Gfx;
use crate::view::fish::FishBoss;
use glam::Vec3;
use sim::battle::{Boss, BossState};

pub struct BossView {
    fish: FishBoss,
}

impl BossView {
    pub fn new(gfx: &Gfx) -> Result<Self, String> { Ok(BossView { fish: FishBoss::new(gfx)? }) }

    pub fn pos(&self) -> Vec3 { self.fish.pos }
    /// 跳び上がっている高さ（カメラが見上げるのに使う）
    pub fn hop(&self) -> f32 { self.fish.hop }
    /// 胸びれの位置（left = 左。光と火の粉を出す）
    pub fn hand(&self, left: bool) -> Option<Vec3> { self.fish.fin(left) }
    /// 腹の位置
    pub fn foot(&self, left: bool) -> Option<Vec3> { self.fish.belly(left) }

    /// light = (灯の下からの光, 怒り)。どちらも 0〜1
    pub fn update(&mut self, gfx: &Gfx, b: &Boss, dt: f64, frozen: bool, y: f32, light: (f32, f32)) {
        let hit_t = b.hit_t as f32;
        let flash = if b.state == BossState::Dead { 0.0 } else if hit_t < 0.06 { 0.85 } else { (0.4 - hit_t * 4.0).max(0.0) };
        let angry = if b.hp / b.max_hp < 0.4 { (0.5 + 0.5 * (crate::platform::clock_ms() / 120.0).sin()) as f32 } else { 0.0 };
        self.fish.update(gfx, b, dt, frozen, y, flash, angry + light.1 * 0.3);
    }

    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) { self.fish.shadow(pass); }
    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) { self.fish.draw(gfx, pass, order); }
}
