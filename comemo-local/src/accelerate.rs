use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{MappedRwLockReadGuard, Mutex, RwLock, RwLockReadGuard};
use rustc_hash::FxHashMap;

/// The global list of currently alive accelerators.
static ACCELERATORS: RwLock<(usize, Vec<Accelerator>)> = RwLock::new((0, Vec::new()));

/// The current ID of the accelerator.
static ID: AtomicUsize = AtomicUsize::new(0);

/// The type of each individual accelerator.
///
/// Maps from call hashes to return hashes.
type Accelerator = Mutex<FxHashMap<u128, u128>>;

/// Maximum number of accelerator entries before automatic eviction.
/// Each entry is ~64 bytes, so 100K entries = ~6.4 MB.
/// Without this cap, documents with 1M+ memoize calls grow to ~118 MB.
const MAX_ACCELERATORS: usize = 100_000;

/// Generate a new accelerator.
pub fn id() -> usize {
    // Get the next ID.
    ID.fetch_add(1, Ordering::SeqCst)
}

/// Evict the accelerators.
pub fn evict() {
    let mut accelerators = ACCELERATORS.write();
    let (offset, vec) = &mut *accelerators;

    // Update the offset.
    *offset = ID.load(Ordering::SeqCst);

    // Drop all accelerator entries and free the backing memory.
    *vec = Vec::new();
}

/// Get an accelerator by ID.
pub fn get(id: usize) -> Option<MappedRwLockReadGuard<'static, Accelerator>> {
    // We always lock the accelerators, as we need to make sure that the
    // accelerator is not removed while we are reading it.
    let mut accelerators = ACCELERATORS.read();

    let mut i = id.checked_sub(accelerators.0)?;
    if i >= accelerators.1.len() {
        drop(accelerators);
        resize(i + 1);
        accelerators = ACCELERATORS.read();

        // Because we release the lock before resizing the accelerator, we need
        // to check again whether the ID is still valid because another thread
        // might evicted the cache.
        i = id.checked_sub(accelerators.0)?;
    }

    Some(RwLockReadGuard::map(accelerators, move |(_, vec)| &vec[i]))
}

/// Adjusts the amount of accelerators.
#[cold]
fn resize(len: usize) {
    let mut pair = ACCELERATORS.write();

    // If the accelerator Vec would exceed the cap, evict and reset.
    // This bounds memory to MAX_ACCELERATORS * ~64 bytes (~6.4 MB)
    // instead of growing unboundedly (~118 MB for 1M+ calls).
    // The accelerator is a performance optimization only — evicting it
    // forces fallback to the slower full-cache lookup path.
    if len > MAX_ACCELERATORS {
        pair.0 = ID.load(Ordering::SeqCst);
        pair.1 = Vec::new();
        return;
    }

    if len > pair.1.len() {
        pair.1.resize_with(len, || Mutex::new(FxHashMap::default()));
    }
}
