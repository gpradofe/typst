//! Layout of content into a [`Document`].

mod collect;
mod finalize;
mod run;

use comemo::{Track, Tracked, TrackedMut};
use ecow::EcoVec;
use typst_library::World;
use typst_library::diag::{At, SourceResult};
use typst_library::engine::{Engine, Route, Sink, Traced};
use typst_library::foundations::{Content, StyleChain};
use typst_library::introspection::{
    Introspector, Locator, LocatorLink, ManualPageCounter, SplitLocator, TagElem,
};
use typst_library::layout::{FrameItem, Point};
use typst_library::model::DocumentInfo;
use typst_library::routines::{Arenas, Pair, RealizationKind, Routines};
use typst_utils::{Numeric, Protected};

use typst_library::foundations::Resolve;
use typst_library::layout::{Abs, ColumnsElem, PageElem, Regions};

use self::collect::{Item, collect};
use self::finalize::finalize;
use self::run::{
    LayoutedPage, create_layouted_page, layout_blank_page, layout_page_run,
    prepare_page_run_no_fragment,
};
use crate::flow::{FlowMode, layout_flow_streaming};
use crate::page_store::DiskPageStore;
use crate::{Page, PagedDocument, PagedIntrospector, PagedIntrospectorBuilder};

/// Layout content into a document.
///
/// This first performs root-level realization and then lays out the resulting
/// elements. In contrast to [`layout_fragment`](crate::layout_fragment),
/// this does not take regions since the regions are defined by the page
/// configuration in the content and style chain.
#[typst_macros::time(name = "layout document")]
pub fn layout_document(
    engine: &mut Engine,
    content: &Content,
    styles: StyleChain,
) -> SourceResult<PagedDocument> {
    layout_document_impl(
        engine.routines,
        engine.world,
        engine.introspector.into_raw(),
        engine.traced,
        TrackedMut::reborrow_mut(&mut engine.sink),
        engine.route.track(),
        content,
        styles,
    )
}

/// The internal implementation of `layout_document`.
// Disable document-level memoization. The cached PagedDocument holds
// all page frames, dominating memory. During convergence, the introspector
// changes every iteration so this cache never hits anyway.
#[comemo::memoize(enabled = false)]
#[allow(clippy::too_many_arguments)]
fn layout_document_impl(
    routines: &Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<dyn Introspector + '_>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    content: &Content,
    styles: StyleChain,
) -> SourceResult<PagedDocument> {
    layout_document_common(
        routines,
        world,
        introspector,
        traced,
        sink,
        route,
        content,
        Locator::root(),
        styles,
    )
}

/// Layout content into a document, as part of a bundle compilation process.
#[typst_macros::time(name = "layout document")]
pub fn layout_document_for_bundle(
    engine: &mut Engine,
    content: &Content,
    locator: Locator,
    styles: StyleChain,
) -> SourceResult<PagedDocument> {
    layout_document_for_bundle_impl(
        engine.routines,
        engine.world,
        engine.introspector.into_raw(),
        engine.traced,
        TrackedMut::reborrow_mut(&mut engine.sink),
        engine.route.track(),
        content,
        locator.track(),
        styles,
    )
}

/// The internal implementation of `layout_document_for_bundle`.
#[comemo::memoize(enabled = false)]
#[allow(clippy::too_many_arguments)]
fn layout_document_for_bundle_impl(
    routines: &Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<dyn Introspector + '_>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    content: &Content,
    locator: Tracked<Locator>,
    styles: StyleChain,
) -> SourceResult<PagedDocument> {
    let link = LocatorLink::new(locator);
    layout_document_common(
        routines,
        world,
        introspector,
        traced,
        sink,
        route,
        content,
        Locator::link(&link),
        styles,
    )
}

/// The shared, unmemoized implementation of `layout_document` and
/// `layout_document_for_bundle`.
#[allow(clippy::too_many_arguments)]
fn layout_document_common(
    routines: &Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<dyn Introspector + '_>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    content: &Content,
    locator: Locator,
    styles: StyleChain,
) -> SourceResult<PagedDocument> {
    let introspector = Protected::from_raw(introspector);
    let mut locator = locator.split();
    let mut engine = Engine {
        routines,
        world,
        introspector,
        traced,
        sink,
        route: Route::extend(route).unnested(),
    };

    // Mark the external styles as "outside" so that they are valid at the page
    // level.
    let styles = styles.to_map().outside();
    let styles = StyleChain::new(&styles);
    let arenas = Arenas::default();

    let mut info = DocumentInfo::default();
    info.populate(styles);
    info.populate_locale(styles);

    let mut children = (engine.routines.realize)(
        RealizationKind::LayoutDocument { info: &mut info },
        &mut engine,
        &mut locator,
        &arenas,
        content,
        styles,
    )?;

    // After realization, eval closure caches (~594 MB for 100K-row tables)
    // are no longer needed — the cellgrid is stored in a separate cache
    // and children hold their own Content references. Evict during
    // iteration 1 to free these caches before layout begins.
    // Use cumulative grid entries (200K threshold) instead of per-table
    // size to avoid penalizing multi-table documents with small tables
    // (stress test: 8 × 17.5K = 140K entries). Evicting here destroys
    // iteration 1 comemo caches, forcing iteration 2 to re-shape all text.
    // For 100K-row tables (300K+ entries), the savings are huge (~600 MB).
    // For stress (140K entries), savings are small (~48 MB) but rebuild
    // cost is high (~40% slowdown).
    // After realization, eval closure caches (~200-600 MB for large tables)
    // are no longer needed. Evict and trim WS for large documents. Threshold:
    // 50K entries in a single grid. This catches 100K-row templates (300K+
    // entries) but exempts stress test (8 × 17.5K = 140K, max grid 17.5K).
    // Stress tables need comemo caches for iteration 2 cache hits, and WS
    // trim would cause page faults in iteration 2.
    // Post-realization eviction ONLY for large single-table documents.
    // For multi-table docs, eviction here can drop Content references that
    // carry grid_meta (needed by PDF tag context). Multi-table docs
    // instead evict between page runs in the streaming path below.
    if typst_library::engine_flags::is_layout_eviction_enabled()
        && typst_library::layout::grid::resolve::has_large_cellgrid(50_000)
    {
        comemo::evict(0);
        typst_library::engine_flags::compact_heap_and_trim_ws_full();
    }

    let (pages, store, introspector) =
        layout_pages_streaming(&mut engine, &mut children, &mut locator, styles)?;

    let mut doc = PagedDocument::new(pages, info);
    if let Some(introspector) = introspector {
        doc.set_introspector(introspector);
    }
    if let Some(store) = store {
        doc.set_page_store(store);
    }
    Ok(doc)
}

/// Page count threshold above which pages are flushed to disk during layout.
/// Below this, all pages are kept in memory (current behavior).
/// Lower values reduce peak RSS but increase disk I/O.
const FLUSH_THRESHOLD: usize = 25;

/// Layout pages with streaming disk flush for large documents.
///
/// For small documents: returns (all pages, None, None) — current behavior.
/// For large documents: returns (empty pages, store, introspector).
///   Pages are serialized to disk as produced. Introspector is built
///   incrementally from each page before the page is dropped. After
///   processing each page run, comemo cache is evicted to free frame data.
fn layout_pages_streaming<'a>(
    engine: &mut Engine,
    children: &'a mut [Pair<'a>],
    locator: &mut SplitLocator<'a>,
    styles: StyleChain<'a>,
) -> SourceResult<(EcoVec<Page>, Option<DiskPageStore>, Option<PagedIntrospector>)> {
    // Slice up the children into logical parts.
    let items = collect(children, locator, styles);

    let mut pages = EcoVec::new();
    let mut tags = vec![];
    let mut counter = ManualPageCounter::new();
    let mut total_pages: usize = 0;

    let streaming = typst_library::engine_flags::is_streaming_mode();
    // Check grid sizes to decide layout strategy.
    let has_large_grid = typst_library::layout::grid::resolve::has_large_cellgrid(50_000);
    // Cumulative counter tracks ALL grid entries processed during realization,
    // even after the CellGrid cache is cleared (MAX_CELLGRID_CACHE = 30).
    // This correctly identifies multi-table documents with many small tables
    // (e.g., 2076 tables × 500 entries = 1M cumulative).
    let cumulative_entries = typst_library::engine_flags::cumulative_grid_entries();
    let run_count = items.iter().filter(|i| matches!(i, Item::Run(..))).count();
    // Layout strategy:
    // - Large single table: streaming (per-page eviction + DiskPageStore)
    // - Many tables in few runs (multi-table template): streaming
    //   All tables are in 1-3 runs, no parallelism benefit from rayon.
    // - Many tables in many runs (stress test): parallel with DiskPageStore
    //   Each department has its own page run (8+ runs). Rayon gives 2-3x
    //   speedup. Pages flushed to disk after parallel results arrive.
    let has_many_tables = cumulative_entries >= 50_000;
    let use_streaming_layout = has_large_grid || (has_many_tables && run_count <= 4);
    // Flush to DiskPageStore for all large documents (streaming or parallel).
    let mut flushing = streaming || use_streaming_layout || has_many_tables;
    let mut store: Option<DiskPageStore> = if flushing {
        Some(
            DiskPageStore::new()
                .map_err(|e| ecow::eco_format!("disk store creation failed: {e}"))
                .at(typst_syntax::Span::detached())?,
        )
    } else {
        None
    };
    // In streaming mode (Phase 2), skip building the introspector — Phase 1's
    // converged introspector is reused instead, saving ~104 MB.
    // For Phase 1, build incrementally during layout as before.
    let mut intro_builder: Option<PagedIntrospectorBuilder> = if flushing && !streaming {
        Some(PagedIntrospectorBuilder::with_capacity(0))
    } else {
        None
    };

    // Shared logic: process a single finalized page (flush or accumulate).
    let process_page = |page: Page,
                        total_pages: &mut usize,
                        pages: &mut EcoVec<Page>,
                        store: &mut Option<DiskPageStore>,
                        intro_builder: &mut Option<PagedIntrospectorBuilder>,
                        flushing: &mut bool|
     -> SourceResult<()> {
        // Start flushing mid-stream if threshold exceeded.
        // Flush in ALL iterations to bound peak memory. Without this,
        // iter2+ accumulates all pages in memory (~27GB at 600K rows),
        // causing heavy swapping on 32GB machines.
        if !*flushing && has_large_grid && *total_pages + 1 > FLUSH_THRESHOLD {
            let mut s = DiskPageStore::new()
                .map_err(|e| ecow::eco_format!("disk store creation failed: {e}"))
                .at(typst_syntax::Span::detached())?;
            let mut ib = PagedIntrospectorBuilder::with_capacity(*total_pages);
            for (i, p) in pages.iter().enumerate() {
                ib.discover_page(i, p);
                s.append_page(p)
                    .map_err(|e| ecow::eco_format!("disk flush failed: {e}"))
                    .at(typst_syntax::Span::detached())?;
            }
            *pages = EcoVec::new();
            *store = Some(s);
            *intro_builder = Some(ib);
            *flushing = true;
        }

        if *flushing {
            if let Some(ib) = intro_builder.as_mut() {
                ib.discover_page(*total_pages, &page);
            }
            store
                .as_mut()
                .unwrap()
                .append_page(&page)
                .map_err(|e| ecow::eco_format!("disk flush failed: {e}"))
                .at(typst_syntax::Span::detached())?;
        } else {
            pages.push(page);
        }
        *total_pages += 1;
        Ok(())
    };

    // For the first convergence iteration, use streaming page-by-page
    // processing. This avoids accumulating all page frames in memory —
    // each page is finalized, flushed to disk, and dropped before the next
    // is produced. For subsequent iterations, use the parallel memoized path.
    if use_streaming_layout {
        for item in &items {
            match item {
                Item::Run(children, initial, run_locator) => {
                    // Prepare page styles without performing flow layout.
                    let prepared =
                        prepare_page_run_no_fragment(engine, children, *initial);

                    // Compute content area from page size and margins.
                    let area = prepared.page_size - prepared.margin.sum_by_axis();
                    let styles = StyleChain::new(&prepared.styles);

                    // Use streaming flow layout: each page frame is composed,
                    // processed (finalized + flushed to disk), and dropped before the
                    // next is composed. This avoids holding all ~500 page
                    // frames simultaneously, saving ~160 MB of peak RAM.
                    let flow_loc = run_locator.relayout();
                    let mut flow_split = flow_loc.split();
                    let mut page_idx = 0usize;
                    let mut last_evict_page: usize = 0;
                    layout_flow_streaming(
                        engine,
                        children,
                        &mut flow_split,
                        styles,
                        Regions::repeat(area, area.map(Abs::is_finite)),
                        styles.get(PageElem::columns),
                        styles.get(ColumnsElem::gutter).resolve(styles),
                        FlowMode::Root,
                        &mut |engine, frame| {
                            let layouted = create_layouted_page(
                                engine,
                                frame,
                                &prepared,
                                locator.next(&page_idx),
                            )?;
                            let page =
                                finalize(engine, &mut counter, &mut tags, layouted)?;
                            process_page(
                                page,
                                &mut total_pages,
                                &mut pages,
                                &mut store,
                                &mut intro_builder,
                                &mut flushing,
                            )?;
                            page_idx += 1;

                            // Periodic comemo eviction during streaming layout.
                            // Shaped text and inline layout caches accumulate
                            // ~2-3 MB per page. Evict periodically to bound
                            // cache growth.
                            // - Large single-table: evict every 5 pages with
                            //   HeapCompact (cheap ~sub-100ms). Full WS trim
                            //   (SetProcessWorkingSetSize, ~500ms+ each call)
                            //   only every 25 pages at major boundaries.
                            //   At every-5 full trim, a 600K-row table with
                            //   1500 pages would burn ~300s on WS trim alone.
                            // - Multi-table streaming: evict every 50 pages.
                            if has_large_grid && flushing {
                                const EVICT_INTERVAL: usize = 5;
                                const TRIM_INTERVAL: usize = 25;
                                if total_pages >= last_evict_page + EVICT_INTERVAL {
                                    comemo::evict(0);
                                    if total_pages / TRIM_INTERVAL
                                        > last_evict_page / TRIM_INTERVAL
                                    {
                                        typst_library::engine_flags::compact_heap_and_trim_ws_full();
                                    } else {
                                        typst_library::engine_flags::compact_heap_and_trim_ws();
                                    }
                                    last_evict_page = total_pages;
                                }
                            } else if use_streaming_layout {
                                // Multi-table docs: evict every 50 pages.
                                // Less aggressive than single-table to preserve
                                // some cross-table cache locality.
                                let evict_interval: usize = 50;
                                if total_pages >= last_evict_page + evict_interval {
                                    comemo::evict(0);
                                    last_evict_page = total_pages;
                                }
                            }

                            Ok(())
                        },
                    )?;

                    // Between-run cleanup when flushing to disk.
                    // - Large single-table: full WS trim + eviction (one run).
                    // - Multi-table streaming: lightweight evict only — frees
                    //   the previous table's comemo cache without the expensive
                    //   SetProcessWorkingSetSize call that causes page faults.
                    if flushing {
                        comemo::evict(0);
                        if has_large_grid {
                            typst_library::engine_flags::compact_heap_and_trim_ws_full();
                        }
                    }
                }
                Item::Parity(parity, initial, par_locator) => {
                    if !parity.matches(total_pages) {
                        continue;
                    }
                    let layouted =
                        layout_blank_page(engine, par_locator.relayout(), *initial)?;
                    let page = finalize(engine, &mut counter, &mut tags, layouted)?;
                    process_page(
                        page,
                        &mut total_pages,
                        &mut pages,
                        &mut store,
                        &mut intro_builder,
                        &mut flushing,
                    )?;
                }
                Item::Tags(items) => {
                    tags.extend(
                        items
                            .iter()
                            .filter_map(|(c, _)| c.to_packed::<TagElem>())
                            .map(|elem| elem.tag.clone()),
                    );
                }
            }
        }
    } else {
        // Chunked parallel page runs via rayon. Used for documents with many
        // page runs (e.g., stress test with 8 departments). Instead of running
        // all runs in parallel (which holds all tables' comemo caches alive
        // simultaneously, ~1.4 GB peak), we process runs in small chunks. Each
        // chunk runs in parallel, then results are flushed to disk and comemo
        // is evicted before the next chunk starts. This caps in-flight memory
        // at CHUNK_SIZE × per-table-cost rather than all-tables × per-table.
        //
        // Wall-time impact: For run_count=8 with CHUNK_SIZE=2 on 32 cores,
        // we trade 8-way parallelism for 2-way. The largest table dominates
        // each chunk's wall time; total = sum of largest in each chunk.
        const CHUNK_SIZE: usize = 2;

        let mut i = 0;
        while i < items.len() {
            if matches!(items[i], Item::Run(..)) {
                // Find consecutive Run items starting at i.
                let mut j = i;
                while j < items.len() && matches!(items[j], Item::Run(..)) {
                    j += 1;
                }
                // Process the run range [i..j] in chunks of CHUNK_SIZE.
                let mut k = i;
                while k < j {
                    let chunk_end = (k + CHUNK_SIZE).min(j);
                    let chunk_args: Vec<_> = items[k..chunk_end]
                        .iter()
                        .map(|item| match item {
                            Item::Run(children, initial, locator) => {
                                (*children, *initial, locator.relayout())
                            }
                            _ => unreachable!(),
                        })
                        .collect();

                    let results: Vec<_> = engine
                        .parallelize(
                            chunk_args.into_iter(),
                            |engine, (children, initial, locator)| {
                                layout_page_run(engine, children, locator, initial)
                            },
                        )
                        .collect();

                    for layouted in results {
                        let layouted = layouted?;
                        for lp in layouted {
                            let page = finalize(engine, &mut counter, &mut tags, lp)?;
                            process_page(
                                page,
                                &mut total_pages,
                                &mut pages,
                                &mut store,
                                &mut intro_builder,
                                &mut flushing,
                            )?;
                        }
                    }

                    // Evict this chunk's comemo caches before the next chunk
                    // starts. Each chunk's tables are fully laid out, finalized,
                    // and flushed to disk by this point — we no longer need
                    // their cached layouts, text shapes, or fragments.
                    // Use full WS trim only on intermediate chunks (not the
                    // last). The next chunk accesses different Content
                    // sub-trees (different departments), so a one-time
                    // re-fault recovers ~200 MB of RSS. The last chunk skips
                    // WS trim since no further layout work follows.
                    let is_last_chunk = chunk_end >= j;
                    if flushing && has_many_tables {
                        comemo::evict(0);
                        if is_last_chunk {
                            typst_library::engine_flags::compact_heap_and_trim_ws();
                        } else {
                            typst_library::engine_flags::compact_heap_and_trim_ws_full();
                        }
                    }

                    k = chunk_end;
                }
                i = j;
            } else {
                match &items[i] {
                    Item::Parity(parity, initial, par_locator) => {
                        if parity.matches(total_pages) {
                            let layouted = layout_blank_page(
                                engine,
                                par_locator.relayout(),
                                *initial,
                            )?;
                            let page =
                                finalize(engine, &mut counter, &mut tags, layouted)?;
                            process_page(
                                page,
                                &mut total_pages,
                                &mut pages,
                                &mut store,
                                &mut intro_builder,
                                &mut flushing,
                            )?;
                        }
                    }
                    Item::Tags(tag_items) => {
                        tags.extend(
                            tag_items
                                .iter()
                                .filter_map(|(c, _)| c.to_packed::<TagElem>())
                                .map(|elem| elem.tag.clone()),
                        );
                    }
                    Item::Run(..) => unreachable!(),
                }
                i += 1;
            }
        }
    }

    // Flush buffered writes before any reads from the store.
    if let Some(s) = store.as_mut() {
        s.flush_writer()
            .map_err(|e| ecow::eco_format!("disk store flush failed: {e}"))
            .at(typst_syntax::Span::detached())?;
    }

    // Add remaining tags to the last page.
    if !tags.is_empty() {
        if let Some(s) = store.as_mut() {
            // Discover remaining tags in the introspector before finishing.
            if let Some(ib) = intro_builder.as_mut() {
                // Use height of last page for tag position. We need to read
                // it from the store since pages vec is empty when flushing.
                let page_height = if total_pages > 0 {
                    // Read last page just for its height, then drop it.
                    s.read_page(total_pages - 1)
                        .map(|p| p.frame.height())
                        .unwrap_or_default()
                } else {
                    Abs::zero()
                };
                ib.discover_remaining_tags(
                    total_pages.saturating_sub(1),
                    &tags,
                    page_height,
                );
            }
            // Store remaining tags so they're injected when reading the last page.
            s.set_remaining_tags(tags);
        } else if let Some(last) = pages.make_mut().last_mut() {
            let pos = Point::with_y(last.frame.height());
            last.frame
                .push_multiple(tags.into_iter().map(|tag| (pos, FrameItem::Tag(tag))));
        }
    }

    // Build the introspector if we were building incrementally.
    let introspector = intro_builder.map(|ib| ib.finish_incremental(total_pages));

    Ok((pages, store, introspector))
}
