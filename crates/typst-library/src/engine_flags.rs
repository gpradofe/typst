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

/// Cumulative grid entry counter. Tracks total entries across all grids
/// in the current layout pass. Used to trigger table-level memoize bypass
/// for multi-table documents where individual tables are small (<500 entries)
/// but collectively grow the comemo cache to ~165 MB of ShapedText.
static CUMULATIVE_GRID_ENTRIES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Add entries to the cumulative grid counter. Returns the new total.
pub fn add_grid_entries(count: usize) -> usize {
    CUMULATIVE_GRID_ENTRIES.fetch_add(count, Ordering::Relaxed) + count
}

/// Reset the cumulative grid entry counter (between iterations).
pub fn reset_grid_entries() {
    CUMULATIVE_GRID_ENTRIES.store(0, Ordering::Relaxed);
}

/// Get the current cumulative grid entry count.
pub fn cumulative_grid_entries() -> usize {
    CUMULATIVE_GRID_ENTRIES.load(Ordering::Relaxed)
}

/// Whether table-level layout_fragment_impl should bypass memoization.
/// Set by GridLayouter when cumulative entries are high, enabling
/// selective bypass for multi-table documents during iteration 1.
/// Unlike CELL_MEMOIZE_BYPASS, this is set BEFORE the table's
/// layout_fragment_impl call (via flow collect or grid layout entry).
static TABLE_LEVEL_BYPASS: AtomicBool = AtomicBool::new(false);

/// Enable table-level memoize bypass.
pub fn enable_table_level_bypass() {
    TABLE_LEVEL_BYPASS.store(true, Ordering::Relaxed);
}

/// Disable table-level memoize bypass.
pub fn disable_table_level_bypass() {
    TABLE_LEVEL_BYPASS.store(false, Ordering::Relaxed);
}

/// Check if table-level memoize bypass is active.
pub fn is_table_level_bypassed() -> bool {
    TABLE_LEVEL_BYPASS.load(Ordering::Relaxed)
}

/// Threshold for cumulative grid entries beyond which table caching is
/// disabled and aggressive eviction (evict(0)) is used between iterations.
/// At 600K rows (~6M entries), tables should still get cached for iter2+
/// cache hits. At 1.2M rows (~12M entries), comemo entries from different
/// iterations accumulate to 10+ GB, so evict(0) is needed.
/// Set to 8M to exempt 600K while catching 1.2M+.
pub const TABLE_CACHE_ENTRY_LIMIT: usize = 8_000_000;

/// Compact the heap on Windows without trimming the working set.
///
/// After freeing large allocations (e.g., comemo eviction), the Windows heap
/// retains freed pages. `_heapmin` returns free CRT blocks to the OS and
/// `HeapCompact` coalesces free blocks. Unlike `compact_heap_and_trim_ws_full`,
/// this does NOT call `SetProcessWorkingSetSize`, avoiding expensive page
/// faults on actively-used memory (e.g., the 267 MB eval Content tree).
///
/// On non-Windows platforms, this is a no-op.
#[inline]
pub fn compact_heap_and_trim_ws() {
    #[cfg(windows)]
    unsafe {
        unsafe extern "system" {
            fn GetProcessHeap() -> *mut core::ffi::c_void;
            fn HeapCompact(heap: *mut core::ffi::c_void, flags: u32) -> usize;
        }
        unsafe extern "C" {
            fn _heapmin() -> i32;
        }
        let _ = _heapmin();
        HeapCompact(GetProcessHeap(), 0);
    }
}

/// Full heap compaction + working set trim on Windows.
///
/// Calls `_heapmin`, `HeapCompact`, and `SetProcessWorkingSetSize` to
/// aggressively release memory back to the OS. Only use at major boundaries
/// (post-eval, between convergence iterations) where the subsequent work
/// pattern accesses different memory — NOT during layout where the Content
/// tree is continuously accessed.
///
/// On non-Windows platforms, this is a no-op.
#[inline]
pub fn compact_heap_and_trim_ws_full() {
    #[cfg(windows)]
    unsafe {
        unsafe extern "system" {
            fn GetProcessHeap() -> *mut core::ffi::c_void;
            fn HeapCompact(heap: *mut core::ffi::c_void, flags: u32) -> usize;
            fn GetCurrentProcess() -> *mut core::ffi::c_void;
            fn SetProcessWorkingSetSize(
                process: *mut core::ffi::c_void,
                min: usize,
                max: usize,
            ) -> i32;
        }
        unsafe extern "C" {
            fn _heapmin() -> i32;
        }
        let _ = _heapmin();
        HeapCompact(GetProcessHeap(), 0);
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    // These flags are process-global, so every test that touches them must
    // run under the same serial guard to avoid stomping on the others.
    fn guard() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn layout_eviction_toggles() {
        let _g = guard();
        enable_layout_eviction();
        assert!(is_layout_eviction_enabled());
        disable_layout_eviction();
        assert!(!is_layout_eviction_enabled());
        enable_layout_eviction();
        assert!(is_layout_eviction_enabled());
    }

    #[test]
    fn streaming_mode_toggles() {
        let _g = guard();
        disable_streaming_mode();
        assert!(!is_streaming_mode());
        enable_streaming_mode();
        assert!(is_streaming_mode());
        disable_streaming_mode();
        assert!(!is_streaming_mode());
    }

    #[test]
    fn cell_memoize_bypass_toggles() {
        let _g = guard();
        disable_cell_memoize_bypass();
        assert!(!is_cell_memoize_bypassed());
        enable_cell_memoize_bypass();
        assert!(is_cell_memoize_bypassed());
        disable_cell_memoize_bypass();
        assert!(!is_cell_memoize_bypassed());
    }

    #[test]
    fn table_level_bypass_toggles() {
        let _g = guard();
        disable_table_level_bypass();
        assert!(!is_table_level_bypassed());
        enable_table_level_bypass();
        assert!(is_table_level_bypassed());
        disable_table_level_bypass();
        assert!(!is_table_level_bypassed());
    }

    #[test]
    fn grid_entry_counter_accumulates_and_resets() {
        let _g = guard();
        reset_grid_entries();
        assert_eq!(cumulative_grid_entries(), 0);

        assert_eq!(add_grid_entries(100), 100);
        assert_eq!(add_grid_entries(250), 350);
        assert_eq!(cumulative_grid_entries(), 350);

        reset_grid_entries();
        assert_eq!(cumulative_grid_entries(), 0);

        assert_eq!(add_grid_entries(1), 1);
        reset_grid_entries();
    }

    #[test]
    fn flags_do_not_interfere_with_each_other() {
        let _g = guard();
        disable_layout_eviction();
        disable_streaming_mode();
        disable_cell_memoize_bypass();
        disable_table_level_bypass();

        enable_streaming_mode();
        assert!(is_streaming_mode());
        assert!(!is_layout_eviction_enabled());
        assert!(!is_cell_memoize_bypassed());
        assert!(!is_table_level_bypassed());

        enable_cell_memoize_bypass();
        assert!(is_streaming_mode());
        assert!(is_cell_memoize_bypassed());
        assert!(!is_table_level_bypassed());

        disable_streaming_mode();
        disable_cell_memoize_bypass();
    }

    #[test]
    fn heap_compact_helpers_do_not_panic() {
        // On non-Windows these are no-ops; on Windows they must execute
        // the underlying CRT/WinAPI calls cleanly. Either way the
        // function must return without panicking.
        let _g = guard();
        compact_heap_and_trim_ws();
        compact_heap_and_trim_ws_full();
    }
}
