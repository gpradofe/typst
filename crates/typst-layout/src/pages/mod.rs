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
    // iteration 1 to free these caches before layout begins. Only for
    // large tables (≥100K entries) where the savings are significant.
    // Small documents skip this to preserve measurement caches.
    if typst_library::engine_flags::is_layout_eviction_enabled()
        && typst_library::layout::grid::resolve::has_large_cellgrid(100_000)
    {
        comemo::evict(0);
    }

    let (pages, store, introspector) =
        layout_pages_streaming(&mut engine, &mut children, &mut locator, styles)?;

    let mut doc = PagedDocument::new(pages, info);
    if let Some(introspector) = introspector {
        // For large documents: introspector was built incrementally
        // during layout. Set it directly instead of lazy-building from pages.
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
    // Always use streaming (per-page callback) path: each page frame is
    // composed, finalized, and flushed to disk before the next is composed.
    // This avoids accumulating all page frames simultaneously in
    // layout_flow's `finished: Vec<Frame>`, which dominates peak RAM for
    // large table documents. Page-run memoization is already disabled
    // (enabled = false), so the parallel path provides no caching benefit.
    let first_iteration = true;

    // Flush pages to disk when: streaming mode (Phase 2), OR during the
    // first convergence iteration for large documents. In both cases we
    // build the introspector incrementally and drop each page after
    // serializing it.
    let mut flushing = streaming;
    let mut store: Option<DiskPageStore> = if flushing {
        Some(
            DiskPageStore::new()
                .map_err(|e| ecow::eco_format!("disk store creation failed: {e}"))
                .at(typst_syntax::Span::detached())?,
        )
    } else {
        None
    };
    let mut intro_builder: Option<PagedIntrospectorBuilder> =
        if flushing { Some(PagedIntrospectorBuilder::with_capacity(0)) } else { None };

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
        if !*flushing && *total_pages + 1 > FLUSH_THRESHOLD {
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
            intro_builder.as_mut().unwrap().discover_page(*total_pages, &page);
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
    if first_iteration {
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
                            // For multi-table documents, small tables never trigger
                            // grid-level eviction (entries < 5000), but their comemo
                            // cache entries accumulate unboundedly across pages.
                            // Evict every 50 pages to bound cache growth while
                            // preserving enough locality for complex table styling.
                            // At 50 pages (~10-15 small tables), comemo accumulates
                            // ~30-50 MB; evicting keeps peak bounded at ~100 MB
                            // for the cache portion.
                            const PAGE_EVICT_INTERVAL: usize = 50;
                            if flushing
                                && total_pages >= last_evict_page + PAGE_EVICT_INTERVAL
                            {
                                comemo::evict(0);
                                last_evict_page = total_pages;
                            }

                            Ok(())
                        },
                    )?;
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
        // Subsequent iterations: use parallel memoized page runs.
        let mut runs = engine.parallelize(
            items.iter().filter_map(|item| match item {
                Item::Run(children, initial, locator) => {
                    Some((children, initial, locator.relayout()))
                }
                _ => None,
            }),
            |engine, (children, initial, locator)| {
                layout_page_run(engine, children, locator, *initial)
            },
        );

        for item in &items {
            match item {
                Item::Run(..) => {
                    let layouted = runs.next().unwrap()?;
                    for layouted in layouted {
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

                    // Evict comemo caches when pages are flushed in
                    // streaming mode. During convergence, caches are
                    // needed for iter2 cache hits.
                    if flushing && streaming {
                        comemo::evict(0);
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
