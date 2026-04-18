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
                    let _ = writeln!(out, "[typst {:>5.2}s] stage: {}", elapsed, name);
                }
                Event::Iteration(n) => {
                    let _ = writeln!(
                        out,
                        "[typst {:>5.2}s] layout iteration {}",
                        elapsed, n
                    );
                }
                Event::Pages(n) => {
                    let _ = writeln!(
                        out,
                        "[typst {:>5.2}s] layout converged, {} page(s)",
                        elapsed, n
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
                            "[typst {:>5.2}s] export {}/{}",
                            elapsed, done, total
                        );
                        state.last_page_logged = *done;
                    }
                }
                Event::Wrote { bytes } => {
                    let _ = writeln!(
                        out,
                        "[typst {:>5.2}s] wrote output ({})",
                        elapsed,
                        human_bytes(*bytes),
                    );
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
                    "[progress {:>5.2}s] {:>3}% {}",
                    elapsed, percent, suffix,
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
        Event::Iteration(n) => format!("layout iteration {}", n),
        Event::Pages(n) => format!("layout converged, {} page(s)", n),
        Event::PageEmitted { done, total } => format!("export {}/{}", done, total),
        Event::Wrote { bytes } => format!("wrote ({})", human_bytes(*bytes)),
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
