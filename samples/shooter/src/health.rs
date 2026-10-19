use oddity::prelude::*;

#[script(id = "9c2e5b10")]
pub struct Health {
    #[default(100.0)]
    pub max: f32,
    /// Fired once when health reaches zero. Listeners are added in the Inspector.
    pub on_died: Event,
    /// Fired on every hit, with the damage taken.
    pub on_damaged: Event<f32>,
    current: f32,
}

impl Script for Health {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.current = self.max;
    }
}

impl Health {
    pub fn damage(&mut self, amount: f32, ctx: &mut Ctx) {
        if self.current <= 0.0 {
            return;
        }
        self.current -= amount;
        self.on_damaged.invoke(ctx, amount);
        if self.current <= 0.0 {
            self.on_died.invoke(ctx);
        }
    }
}
