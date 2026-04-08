//! Global flags for controlling optimization behavior during compilation.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether memory-saving eviction should be performed during layout.
/// Set to true during the first convergence iteration (when we can tolerate
/// cache misses) and false during subsequent iterations (where cache hits
/// from the first iteration speed up validation).
static EVICTION_ENABLED: AtomicBool = AtomicBool::new(true);

/// Enable layout-time cache eviction (first convergence iteration).
pub fn enable_layout_eviction() {
    EVICTION_ENABLED.store(true, Ordering::Relaxed);
}

/// Disable layout-time cache eviction (subsequent convergence iterations).
pub fn disable_layout_eviction() {
    EVICTION_ENABLED.store(false, Ordering::Relaxed);
}

/// Check if layout-time eviction is currently enabled.
pub fn is_layout_eviction_enabled() -> bool {
    EVICTION_ENABLED.load(Ordering::Relaxed)
}

/// Whether streaming (non-memoized) layout mode is active.
/// When true, all `#[comemo::memoize]` layout functions bypass their cache.
/// Set during Phase 2 of two-phase compilation, after convergence.
/// Must be AtomicBool (not thread-local) because engine.parallelize() uses rayon.
static STREAMING_MODE: AtomicBool = AtomicBool::new(false);

/// Enable streaming layout mode (Phase 2: no memoization).
pub fn enable_streaming_mode() {
    STREAMING_MODE.store(true, Ordering::Relaxed);
}

/// Disable streaming layout mode (back to normal memoized layout).
pub fn disable_streaming_mode() {
    STREAMING_MODE.store(false, Ordering::Relaxed);
}

/// Check if streaming mode is currently active.
pub fn is_streaming_mode() -> bool {
    STREAMING_MODE.load(Ordering::Relaxed)
}

/// Whether cell memoization bypass is active.
/// When true, layout functions called during grid cell layout skip
/// comemo caching. This prevents comemo's internal tracking tree from
/// growing to ~1 GB+ for large tables (100K+ cells), since each cell
/// layout is unique and cache hits are essentially 0%.
/// The grid layouter manages its own simple cell cache instead.
static CELL_MEMOIZE_BYPASS: AtomicBool = AtomicBool::new(false);

/// Enable cell memoization bypass (during grid cell layout).
pub fn enable_cell_memoize_bypass() {
    CELL_MEMOIZE_BYPASS.store(true, Ordering::Relaxed);
}

/// Disable cell memoization bypass (normal layout).
pub fn disable_cell_memoize_bypass() {
    CELL_MEMOIZE_BYPASS.store(false, Ordering::Relaxed);
}

/// Check if cell memoization bypass is active.
pub fn is_cell_memoize_bypassed() -> bool {
    CELL_MEMOIZE_BYPASS.load(Ordering::Relaxed)
}
