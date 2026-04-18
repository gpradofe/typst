//! Progress reporting plumbing.
//!
//! A driver (e.g. the CLI) installs a [`Sink`] via [`install`]. Internal
//! compilation and export code then calls [`report`] at known milestones.
//! When no sink is installed, [`report`] is a no-op.

use std::sync::{Arc, RwLock};

/// A milestone during compilation or export.
#[derive(Clone, Debug)]
pub enum Event {
    /// A named stage has just begun.
    Stage(&'static str),
    /// A new convergence/layout iteration has started (1-based).
    Iteration(u32),
    /// Layout has converged; total pages are now known.
    Pages(usize),
    /// A single page has been emitted during export.
    PageEmitted { done: usize, total: usize },
    /// Output has been fully written to disk.
    Wrote { bytes: u64 },
}

/// Destination for progress events.
pub trait Sink: Send + Sync {
    fn report(&self, event: Event);
}

static SINK: RwLock<Option<Arc<dyn Sink>>> = RwLock::new(None);

/// Install (or replace) the global progress sink.
pub fn install(sink: Arc<dyn Sink>) {
    *SINK.write().unwrap() = Some(sink);
}

/// Remove the installed sink, if any.
pub fn clear() {
    *SINK.write().unwrap() = None;
}

/// Forward an event to the installed sink, if any.
pub fn report(event: Event) {
    let sink = SINK.read().unwrap().clone();
    if let Some(sink) = sink {
        sink.report(event);
    }
}
