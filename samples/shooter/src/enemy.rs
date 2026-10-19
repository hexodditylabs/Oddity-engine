use oddity::prelude::*;

use crate::{Health, Player};

#[signal]
pub struct EnemyKilled {
    pub score: u32,
}

#[script(id = "e7c2d909")]
pub struct Enemy {
    #[default(3.0)]
    pub speed: f32,
    #[default(20.0)]
    pub damage_per_second: f32,
    #[default(10)]
    pub score: u32,
    pub target: NodeRef,
}

impl Script for Enemy {
    fn _onready(&mut self, ctx: &mut Ctx) {
        // Wired from code here; it could equally be wired in the Inspector.
        let me = ctx.node();
        ctx.connect(me, "on_died", "die");
        if self.target.is_none() {
            self.target = ctx.all::<Player>().first().copied().unwrap_or_default();
        }
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let Some(target) = ctx.position_of(self.target) else {
            return;
        };
        let to_target = (target - ctx.position()).with_y(0.0);
        if to_target.length() > 1.0 {
            ctx.translate(to_target.normalize() * self.speed * delta);
            let ahead = ctx.position() + to_target;
            ctx.look_at(ahead);
        } else {
            let damage = self.damage_per_second * delta;
            ctx.call::<Health>(self.target, move |health, ctx| health.damage(damage, ctx));
        }
    }
}

#[script]
impl Enemy {
    #[listener]
    fn die(&mut self, ctx: &mut Ctx) {
        ctx.emit(EnemyKilled { score: self.score });
        ctx.destroy();
    }
}
