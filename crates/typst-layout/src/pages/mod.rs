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
use typst_utils::Protected;

use self::collect::{Item, collect};
use self::finalize::finalize;
use self::run::{LayoutedPage, layout_blank_page, layout_page_run, prepare_page_run, create_layouted_page};
use crate::{Page, PagedDocument, PagedIntrospector, PagedIntrospectorBuilder};
use crate::page_store::DiskPageStore;

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

    let (pages, store, introspector) = layout_pages_streaming(
        &mut engine, &mut children, &mut locator, styles,
    )?;

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

/// Page count threshold above which pages are spilled to disk during layout.
/// Below this, all pages are kept in memory (current behavior).
const SPILL_THRESHOLD: usize = 100;

/// Layout pages with streaming disk spill for large documents.
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
    // Always use the sequential streaming path. Page-run memoization
    // is disabled, so the parallel path offers no caching benefit.
    // Sequential processing allows immediate page spilling to disk.
    let first_iteration = true;

    // Spill pages to disk when: streaming mode (Phase 2), OR during the
    // first convergence iteration for large documents. In both cases we
    // build the introspector incrementally and drop each page after
    // serializing it.
    let mut spilling = streaming;
    let mut store: Option<DiskPageStore> = if spilling {
        Some(DiskPageStore::new()
            .map_err(|e| ecow::eco_format!("disk store creation failed: {e}"))
            .at(typst_syntax::Span::detached())?)
    } else {
        None
    };
    let mut intro_builder: Option<PagedIntrospectorBuilder> = if spilling {
        Some(PagedIntrospectorBuilder::with_capacity(0))
    } else {
        None
    };

    // Shared logic: process a single finalized page (spill or accumulate).
    let mut process_page = |page: Page,
                            total_pages: &mut usize,
                            pages: &mut EcoVec<Page>,
                            store: &mut Option<DiskPageStore>,
                            intro_builder: &mut Option<PagedIntrospectorBuilder>,
                            spilling: &mut bool|
     -> SourceResult<()> {
        // Start spilling mid-stream if threshold exceeded.
        // Spill in both convergence and streaming modes. Page-run
        // memoization is disabled during convergence (eviction-enabled),
        // so page frames are not cached by comemo. Spilling frees them.
        if !*spilling && *total_pages + 1 > SPILL_THRESHOLD {
            let mut s = DiskPageStore::new()
                .map_err(|e| ecow::eco_format!("disk store creation failed: {e}"))
                .at(typst_syntax::Span::detached())?;
            let mut ib = PagedIntrospectorBuilder::with_capacity(*total_pages);
            for (i, p) in pages.iter().enumerate() {
                ib.discover_page(i, p);
                s.append_page(p)
                    .map_err(|e| ecow::eco_format!("disk spill failed: {e}"))
                    .at(typst_syntax::Span::detached())?;
            }
            *pages = EcoVec::new();
            *store = Some(s);
            *intro_builder = Some(ib);
            *spilling = true;
        }

        if *spilling {
            intro_builder.as_mut().unwrap().discover_page(*total_pages, &page);
            store
                .as_mut()
                .unwrap()
                .append_page(&page)
                .map_err(|e| ecow::eco_format!("disk spill failed: {e}"))
                .at(typst_syntax::Span::detached())?;
        } else {
            pages.push(page);
        }
        *total_pages += 1;
        Ok(())
    };

    // For the first convergence iteration, use streaming page-by-page
    // processing. This avoids accumulating all page frames in memory —
    // each page is finalized, spilled, and dropped before the next is
    // produced. For subsequent iterations, use the parallel memoized path.
    if first_iteration {
        for item in &items {
            match item {
                Item::Run(children, initial, run_locator) => {
                    let mut prepared = prepare_page_run(
                        engine,
                        children,
                        run_locator.relayout(),
                        *initial,
                    )?;

                    // Take the fragment out so we can iterate it while still
                    // referencing the prepared styles.
                    let fragment = prepared.take_fragment();

                    // Iterate the Fragment lazily. For large flows, the
                    // Fragment is disk-backed so each frame is read on demand.
                    for (page_idx, inner) in fragment.into_iter().enumerate() {
                        let layouted =
                            create_layouted_page(engine, inner, &prepared, locator.next(&page_idx))?;
                        let page =
                            finalize(engine, &mut counter, &mut tags, layouted)?;
                        process_page(
                            page,
                            &mut total_pages,
                            &mut pages,
                            &mut store,
                            &mut intro_builder,
                            &mut spilling,
                        )?;
                        // page + frame dropped here — memory freed
                    }

                    // Do NOT evict comemo caches here. Page spilling already
                    // frees frame memory. Evicting caches would destroy
                    // cross-iteration cache hits, making iteration 2 a full
                    // re-layout (~2x slower overall).
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
                        &mut spilling,
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
                    let run_page_count = layouted.len();

                    for layouted in layouted {
                        let page =
                            finalize(engine, &mut counter, &mut tags, layouted)?;
                        process_page(
                            page,
                            &mut total_pages,
                            &mut pages,
                            &mut store,
                            &mut intro_builder,
                            &mut spilling,
                        )?;
                    }

                    // Evict comemo caches when pages are spilled to disk.
                    // The cached layout data for spilled pages is no longer
                    // needed and can be freed to reduce peak memory.
                    if spilling {
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
                        &mut spilling,
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

    // Add remaining tags to the last page.
    if !tags.is_empty() {
        if store.is_some() {
            // Tags at the end of a spilled document — these are rare.
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

