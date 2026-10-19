use oddity::prelude::*;

#[script(id = "4f9d2c71")]
pub struct Player {
    #[default(6.0)]
    pub speed: f32,
    #[default(0.25)]
    pub fire_rate: f32, // seconds between shots
    pub bullet: Prefab,  // dragged in from the FileSystem
    pub muzzle: NodeRef, // dragged in from the Hierarchy
    cooldown: f32,       // private: runtime only
}

impl Script for Player {
    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let movement = ctx.input().axis2("move"); // WASD or left stick
        let step = Vec3::new(movement.x, 0.0, -movement.y) * self.speed * delta;
        ctx.translate(step);
        if step != Vec3::ZERO {
            let ahead = ctx.position() + step;
            ctx.look_at(ahead); // face where you go
        }

        self.cooldown -= delta;
        if ctx.input().action_pressed("fire") && self.cooldown <= 0.0 {
            self.cooldown = self.fire_rate;
            let at = ctx
                .global_transform_of(self.muzzle)
                .unwrap_or(ctx.global_transform());
            ctx.instantiate(&self.bullet, at);
        }
    }
}
