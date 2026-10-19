use oddity::prelude::*;

#[script(id = "c4a0f6e2")]
pub struct CameraFollow {
    pub target: NodeRef,
    #[default(Vec3::new(0.0, 10.0, 8.0))]
    pub offset: Vec3,
    #[default(5.0)]
    pub smoothing: f32,
}

impl Script for CameraFollow {
    // Late, so it follows where everything ended up this frame.
    fn _onlateprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let Some(target) = ctx.position_of(self.target) else {
            return;
        };
        let t = 1.0 - (-self.smoothing * delta).exp();
        let position = ctx.position().lerp(target + self.offset, t);
        ctx.set_position(position);
        ctx.look_at(target);
    }
}
