//! A10: what a script logs carries its node, its entity and its script, in
//! a `script` span, so Output can show and select the node. A script called
//! from another logs under its own node.

use std::{
    fmt::{Debug, Write as _},
    sync::{Arc, Mutex},
};

use bevy::{
    log::{
        tracing::{
            self, Subscriber,
            field::{Field, Visit},
            span,
        },
        tracing_subscriber::{
            Registry,
            layer::{Context, Layer},
            prelude::*,
            registry::LookupSpan,
        },
    },
    prelude::*,
};
use oddity_script::{Ctx, NodeRef, Script, ScriptPlugin, script};

/// Each log line as `span{fields}: ... message`.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

struct SpanFields(String);

#[derive(Default)]
struct Text(String);

impl Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            write!(self.0, "{value:?}").unwrap();
        } else {
            write!(self.0, " {}={value:?}", field.name()).unwrap();
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Lines {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut text = Text::default();
        attrs.record(&mut text);
        let span = ctx.span(id).unwrap();
        span.extensions_mut().insert(SpanFields(text.0));
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut line = String::new();
        for span in ctx
            .event_scope(event)
            .into_iter()
            .flat_map(|s| s.from_root())
        {
            let extensions = span.extensions();
            let fields = &extensions.get::<SpanFields>().unwrap().0;
            write!(line, "{}{{{}}}: ", span.name(), fields.trim()).unwrap();
        }
        let mut text = Text::default();
        event.record(&mut text);
        line.push_str(&text.0);
        self.0.lock().unwrap().push(line);
    }
}

#[script]
struct Player;

impl Script for Player {}

impl Player {
    fn hurt(&mut self) {
        warn!("ouch");
    }
}

#[script]
struct Goblin {
    pub target: NodeRef,
}

impl Script for Goblin {
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        info!("attacking");
        ctx.call::<Player>(self.target, |player, _ctx| player.hurt());
    }
}

fn plain_system() {
    info!("not a script");
}

#[test]
fn script_logs_carry_their_node() {
    let lines = Lines::default();
    tracing::subscriber::set_global_default(Registry::default().with(lines.clone())).unwrap();

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ScriptPlugin))
        .add_systems(Update, plain_system);
    let world = app.world_mut();
    let player = world.spawn((Name::new("Hero"), Player)).id();
    let goblin = world
        .spawn((
            Name::new("Goblin"),
            Goblin {
                target: player.into(),
            },
        ))
        .id();
    app.update();

    let lines = lines.0.lock().unwrap().clone();
    let goblin_span = format!("script{{node=Goblin entity={goblin} script=logging::Goblin}}: ");
    let hero_span = format!("script{{node=Hero entity={player} script=logging::Player}}: ");
    assert!(
        lines.contains(&format!("{goblin_span}attacking")),
        "{lines:#?}"
    );
    assert!(
        lines.contains(&format!("{goblin_span}{hero_span}ouch")),
        "the called script logs under its own node, inside its caller: {lines:#?}"
    );
    assert!(lines.contains(&"not a script".to_owned()), "{lines:#?}");
}
