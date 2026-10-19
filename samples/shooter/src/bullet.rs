use oddity::prelude::*;

use crate::Health;

#[script(id = "b3e8a514")]
pub struct Bullet {
    #[default(20.0)]
    pub speed: f32,
    #[default(10.0)]
    pub damage: f32,
}

impl Script for Bullet {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.after(2.0, "expire"); // Unity's Invoke
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let forward = ctx.forward();
        ctx.translate(forward * self.speed * delta);

        // Until physics (task F): a hit is an enemy within reach.
        let here = ctx.global_transform().translation();
        let hit = ctx.in_group("enemies").into_iter().find(|&enemy| {
            ctx.position_of(enemy)
                .is_some_and(|at| at.distance(here) < 0.8)
        });
        if let Some(enemy) = hit {
            let damage = self.damage;
            ctx.call::<Health>(enemy, move |health, ctx| health.damage(damage, ctx));
            ctx.destroy();
        }
    }
}

#[script]
impl Bullet {
    #[listener]
    fn expire(&mut self, ctx: &mut Ctx) {
        ctx.destroy();
    }
}
