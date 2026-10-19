//! The way from the MCP server's async tasks into the Bevy world.
//!
//! `rmcp` runs on Tokio, but commands must run on the `World`, in order with
//! the editor's own edits. So each request is a closure sent over a channel
//! and run by an exclusive system at the next update; its answer comes back
//! on a one-shot channel. Every [`CommandExecuted`] goes the other way, so
//! the server can tell the agent what changed.

use std::sync::Mutex;

use bevy_ecs::{message::MessageReader, resource::Resource, system::Res, world::World};
use oddity_core::CommandExecuted;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

/// Work for the world, sent from the server.
type Job = Box<dyn FnOnce(&mut World) + Send>;

/// The server's handle on the world. Cheap to clone.
#[derive(Resource, Clone)]
pub struct WorldBridge {
    jobs: mpsc::UnboundedSender<Job>,
}

impl WorldBridge {
    /// Run `job` on the world at its next update and wait for its answer.
    /// `None` once the app has stopped.
    pub async fn run<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut World) -> R + Send + 'static,
    ) -> Option<R> {
        let (answer, answered) = oneshot::channel();
        self.jobs
            .send(Box::new(move |world| {
                let _ = answer.send(job(world));
            }))
            .ok()?;
        answered.await.ok()
    }
}

/// Jobs waiting for the world. A `Mutex` only because resources must be
/// `Sync`; one system ever reads it.
#[derive(Resource)]
pub(crate) struct Jobs(Mutex<mpsc::UnboundedReceiver<Job>>);

/// Executed commands on their way to the server, as JSON.
#[derive(Resource)]
pub(crate) struct Executed {
    sender: mpsc::UnboundedSender<Value>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
}

impl Executed {
    /// The stream of executed commands. Only one server can have it.
    pub(crate) fn take(&self) -> Option<mpsc::UnboundedReceiver<Value>> {
        self.receiver.lock().ok()?.take()
    }
}

/// The bridge's three resources: the server's handle, the jobs it sends,
/// and the executed commands it reports.
pub(crate) fn resources() -> (WorldBridge, Jobs, Executed) {
    let (jobs, waiting) = mpsc::unbounded_channel();
    let (sender, receiver) = mpsc::unbounded_channel();
    (
        WorldBridge { jobs },
        Jobs(Mutex::new(waiting)),
        Executed {
            sender,
            receiver: Mutex::new(Some(receiver)),
        },
    )
}

/// Run every job that has arrived, in order.
pub(crate) fn run_jobs(world: &mut World) {
    let jobs: Vec<Job> = {
        let Ok(mut waiting) = world.resource::<Jobs>().0.lock() else {
            return;
        };
        std::iter::from_fn(|| waiting.try_recv().ok()).collect()
    };
    for job in jobs {
        job(world);
    }
}

/// Pass every executed command to the server.
pub(crate) fn report_executed(mut executed: MessageReader<CommandExecuted>, out: Res<Executed>) {
    for command in executed.read() {
        let result = match &command.result {
            Ok(output) => json!({ "ok": output }),
            Err(error) => json!({ "error": error.to_string() }),
        };
        // Nobody listening (no server yet, or it stopped) is fine.
        let _ = out.sender.send(json!({
            "id": command.id,
            "args": command.args,
            "source": command.source.to_json(),
            "document": command.document.map(|root| root.to_bits()),
            "result": result,
        }));
    }
}
