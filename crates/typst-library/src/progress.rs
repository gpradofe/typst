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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Collects every event it receives so tests can assert on the sequence.
    struct Collector(Mutex<Vec<Event>>);

    impl Sink for Collector {
        fn report(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn install_collector() -> Arc<Collector> {
        let c = Arc::new(Collector(Mutex::new(Vec::new())));
        install(c.clone());
        c
    }

    // The sink is a process-global, so the tests in this module are
    // serialised by a shared guard to avoid interfering with each other
    // when run under the usual multi-threaded test runner.
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn report_without_sink_is_noop() {
        let _g = guard();
        clear();
        report(Event::Stage("eval"));
        report(Event::Iteration(1));
    }

    #[test]
    fn installed_sink_receives_events_in_order() {
        let _g = guard();
        let c = install_collector();
        report(Event::Stage("eval"));
        report(Event::Iteration(1));
        report(Event::Pages(42));
        report(Event::PageEmitted { done: 1, total: 42 });
        report(Event::Wrote { bytes: 1024 });
        clear();

        let got = c.0.lock().unwrap();
        assert_eq!(got.len(), 5);
        assert!(matches!(got[0], Event::Stage("eval")));
        assert!(matches!(got[1], Event::Iteration(1)));
        assert!(matches!(got[2], Event::Pages(42)));
        assert!(matches!(got[3], Event::PageEmitted { done: 1, total: 42 }));
        assert!(matches!(got[4], Event::Wrote { bytes: 1024 }));
    }

    #[test]
    fn install_replaces_previous_sink() {
        let _g = guard();
        let first = install_collector();
        report(Event::Stage("eval"));
        let second = install_collector();
        report(Event::Stage("layout"));
        clear();

        assert_eq!(first.0.lock().unwrap().len(), 1);
        assert_eq!(second.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn clear_stops_delivering_events() {
        let _g = guard();
        let c = install_collector();
        report(Event::Stage("eval"));
        clear();
        report(Event::Stage("layout"));
        assert_eq!(c.0.lock().unwrap().len(), 1);
    }
}
