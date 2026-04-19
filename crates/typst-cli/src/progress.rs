use std::io::{self, Write};
use std::sync::Mutex;
use std::time::Instant;

use typst::progress::{Event, Sink};

/// Selects which kinds of output lines a [`CliSink`] emits.
#[derive(Copy, Clone, Debug)]
pub enum Mode {
    /// Running percentage only.
    Progress,
    /// Timestamped stage transitions only.
    Verbose,
    /// Both, interleaved.
    Both,
}

/// Stderr-bound sink that formats progress updates for a terminal.
pub struct CliSink {
    mode: Mode,
    start: Instant,
    state: Mutex<State>,
}

struct State {
    last_percent: i16,
    last_page_logged: usize,
}

impl CliSink {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            start: Instant::now(),
            state: Mutex::new(State { last_percent: -1, last_page_logged: 0 }),
        }
    }
}

impl Sink for CliSink {
    fn report(&self, event: Event) {
        let elapsed = self.start.elapsed().as_secs_f32();
        let mut state = self.state.lock().unwrap();

        let verbose = matches!(self.mode, Mode::Verbose | Mode::Both);
        let progress = matches!(self.mode, Mode::Progress | Mode::Both);

        let stderr = io::stderr();
        let mut out = stderr.lock();

        if verbose {
            match &event {
                Event::Stage(name) => {
                    let _ = writeln!(out, "[typst {elapsed:>5.2}s] stage: {name}");
                }
                Event::Iteration(n) => {
                    let _ = writeln!(out, "[typst {elapsed:>5.2}s] layout iteration {n}");
                }
                Event::Pages(n) => {
                    let _ = writeln!(
                        out,
                        "[typst {elapsed:>5.2}s] layout converged, {n} page(s)",
                    );
                }
                Event::PageEmitted { done, total } => {
                    let step = (total / 20).max(50);
                    if *done == 1
                        || *done == *total
                        || done.saturating_sub(state.last_page_logged) >= step
                    {
                        let _ = writeln!(
                            out,
                            "[typst {elapsed:>5.2}s] export {done}/{total}",
                        );
                        state.last_page_logged = *done;
                    }
                }
                Event::Wrote { bytes } => {
                    let human = human_bytes(*bytes);
                    let _ = writeln!(out, "[typst {elapsed:>5.2}s] wrote output ({human})");
                }
            }
        }

        if progress {
            let percent = percent_for(&event);
            let should_print = match &event {
                // Always print stage boundaries and the final write line.
                Event::Stage(_) | Event::Iteration(_) | Event::Pages(_) | Event::Wrote { .. } => {
                    percent as i16 != state.last_percent
                }
                // For page-by-page export, only print when the integer
                // percentage actually advances.
                Event::PageEmitted { .. } => percent as i16 > state.last_percent,
            };
            if should_print {
                state.last_percent = percent as i16;
                let suffix = describe(&event);
                let _ = writeln!(
                    out,
                    "[progress {elapsed:>5.2}s] {percent:>3}% {suffix}",
                );
            }
        }
    }
}

fn percent_for(event: &Event) -> u8 {
    match event {
        Event::Stage("eval") => 5,
        Event::Stage("layout") => 10,
        Event::Stage("export") => 40,
        Event::Stage(_) => 2,
        Event::Iteration(n) => (10 + n.saturating_sub(1).min(4) * 5).min(30) as u8,
        Event::Pages(_) => 40,
        Event::PageEmitted { done, total } => {
            if *total == 0 {
                95
            } else {
                (40 + (done * 55 / total.max(&1)).min(55)) as u8
            }
        }
        Event::Wrote { .. } => 100,
    }
}

fn describe(event: &Event) -> String {
    match event {
        Event::Stage(name) => (*name).to_string(),
        Event::Iteration(n) => format!("layout iteration {n}"),
        Event::Pages(n) => format!("layout converged, {n} page(s)"),
        Event::PageEmitted { done, total } => format!("export {done}/{total}"),
        Event::Wrote { bytes } => format!("wrote ({})", human_bytes(*bytes)),
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        let v = bytes as f64 / GB as f64;
        format!("{v:.1} GB")
    } else if bytes >= MB {
        let v = bytes as f64 / MB as f64;
        format!("{v:.1} MB")
    } else if bytes >= KB {
        let v = bytes as f64 / KB as f64;
        format!("{v:.1} KB")
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_monotonic_across_pipeline() {
        let seq = [
            (Event::Stage("eval"), 5),
            (Event::Stage("layout"), 10),
            (Event::Iteration(1), 10),
            (Event::Iteration(2), 15),
            (Event::Iteration(5), 30),
            (Event::Pages(100), 40),
            (Event::Stage("export"), 40),
            (Event::PageEmitted { done: 1, total: 100 }, 40),
            (Event::PageEmitted { done: 50, total: 100 }, 67),
            (Event::PageEmitted { done: 100, total: 100 }, 95),
            (Event::Wrote { bytes: 0 }, 100),
        ];
        let mut last = 0u8;
        for (event, expected) in seq {
            let got = percent_for(&event);
            assert_eq!(got, expected, "event {event:?}");
            assert!(got >= last, "percent went backwards: {last} -> {got}");
            last = got;
        }
    }

    #[test]
    fn percent_handles_zero_pages_gracefully() {
        let pct = percent_for(&Event::PageEmitted { done: 0, total: 0 });
        assert_eq!(pct, 95);
    }

    #[test]
    fn percent_caps_iteration_weight() {
        assert_eq!(percent_for(&Event::Iteration(100)), 30);
    }

    #[test]
    fn human_bytes_picks_appropriate_unit() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn describe_emits_expected_strings() {
        assert_eq!(describe(&Event::Stage("eval")), "eval");
        assert_eq!(describe(&Event::Iteration(3)), "layout iteration 3");
        assert_eq!(describe(&Event::Pages(42)), "layout converged, 42 page(s)");
        assert_eq!(
            describe(&Event::PageEmitted { done: 10, total: 237 }),
            "export 10/237",
        );
        assert_eq!(
            describe(&Event::Wrote { bytes: 1024 * 1024 }),
            "wrote (1.0 MB)",
        );
    }
}
