use comemo::{Track, Tracked, TrackedMut};
use typst_library::World;
use typst_library::diag::SourceResult;
use typst_library::engine::{Engine, Route, Sink, Traced};
use typst_library::foundations::{
    Content, NativeElement, Resolve, Smart, StyleChain, Styles,
};
use typst_library::introspection::{
    Counter, CounterDisplayElem, CounterKey, Introspector, Locator, LocatorLink,
};
use typst_library::layout::{
    Abs, AlignElem, Alignment, Axes, Binding, ColumnsElem, Dir, Fragment, Frame,
    HAlignment, Length, OuterVAlignment, PageElem, Paper, Region, Regions, Rel, Sides,
    Size, VAlignment,
};
use typst_library::model::Numbering;
use typst_library::pdf::ArtifactKind;
use typst_library::routines::{Pair, Routines};
use typst_library::text::{LocalName, TextElem};
use typst_library::visualize::Paint;
use typst_utils::{Numeric, Protected};

use crate::flow::{FlowMode, layout_flow};

/// A mostly finished layout for one page. Needs only knowledge of its exact
/// page number to be finalized into a `Page`. (Because the margins can depend
/// on the page number.)
#[derive(Clone)]
pub struct LayoutedPage {
    pub inner: Frame,
    pub margin: Sides<Abs>,
    pub binding: Binding,
    pub two_sided: bool,
    pub header: Option<Frame>,
    pub footer: Option<Frame>,
    pub background: Option<Frame>,
    pub foreground: Option<Frame>,
    pub fill: Smart<Option<Paint>>,
    pub numbering: Option<Numbering>,
    pub supplement: Content,
}

/// Layout a single page suitable  for parity adjustment.
pub fn layout_blank_page(
    engine: &mut Engine,
    locator: Locator,
    initial: StyleChain,
) -> SourceResult<LayoutedPage> {
    let layouted = layout_page_run(engine, &[], locator, initial)?;
    Ok(layouted.into_iter().next().unwrap())
}

/// Layout a page run with uniform properties.
#[typst_macros::time(name = "page run")]
pub fn layout_page_run(
    engine: &mut Engine,
    children: &[Pair],
    locator: Locator,
    initial: StyleChain,
) -> SourceResult<Vec<LayoutedPage>> {
    layout_page_run_impl(
        engine.routines,
        engine.world,
        engine.introspector.into_raw(),
        engine.traced,
        TrackedMut::reborrow_mut(&mut engine.sink),
        engine.route.track(),
        children,
        locator.track(),
        initial,
    )
}

/// The internal implementation of `layout_page_run`.
// Disable page-run memoization entirely. The cached Vec<LayoutedPage>
// holds all page frames, dominating memory for large documents.
// Cell-level memoization (layout_fragment_impl, layout_single_impl)
// provides sufficient iter2 speedup without caching full page frames.
#[comemo::memoize(enabled = false)]
#[allow(clippy::too_many_arguments)]
fn layout_page_run_impl(
    routines: &Routines,
    world: Tracked<dyn World + '_>,
    introspector: Tracked<dyn Introspector + '_>,
    traced: Tracked<Traced>,
    sink: TrackedMut<Sink>,
    route: Tracked<Route>,
    children: &[Pair],
    locator: Tracked<Locator>,
    initial: StyleChain,
) -> SourceResult<Vec<LayoutedPage>> {
    let introspector = Protected::from_raw(introspector);
    let link = LocatorLink::new(locator);
    let mut locator = Locator::link(&link).split();
    let mut engine = Engine {
        routines,
        world,
        introspector,
        traced,
        sink,
        route: Route::extend(route),
    };

    // Determine the page-wide styles.
    let styles = Styles::root(children, initial);
    let styles = StyleChain::new(&styles);

    // When one of the lengths is infinite the page fits its content along
    // that axis.
    let width = styles.resolve(PageElem::width).unwrap_or(Abs::inf());
    let height = styles.resolve(PageElem::height).unwrap_or(Abs::inf());
    let mut size = Size::new(width, height);
    if styles.get(PageElem::flipped) {
        std::mem::swap(&mut size.x, &mut size.y);
    }

    let mut min = width.min(height);
    if !min.is_finite() {
        min = Paper::A4.width();
    }

    // Determine the margins.
    let default = Rel::<Length>::from((2.5 / 21.0) * min);
    let margin = styles.get(PageElem::margin);
    let two_sided = margin.two_sided.unwrap_or(false);
    let margin = margin
        .sides
        .map(|side| side.and_then(Smart::custom).unwrap_or(default))
        .resolve(styles)
        .relative_to(size);

    let fill = styles.get_cloned(PageElem::fill);
    let foreground = styles.get_ref(PageElem::foreground);
    let background = styles.get_ref(PageElem::background);
    let header_ascent = styles.resolve(PageElem::header_ascent).relative_to(margin.top);
    let footer_descent =
        styles.resolve(PageElem::footer_descent).relative_to(margin.bottom);
    let numbering = styles.get_ref(PageElem::numbering);
    let supplement = match styles.get_cloned(PageElem::supplement) {
        Smart::Auto => TextElem::packed(PageElem::local_name_in(styles)),
        Smart::Custom(content) => content.unwrap_or_default(),
    };
    let number_align = styles.get(PageElem::number_align);
    let binding = styles.get(PageElem::binding).unwrap_or_else(|| {
        match styles.resolve(TextElem::dir) {
            Dir::LTR => Binding::Left,
            _ => Binding::Right,
        }
    });

    // Construct the numbering (for header or footer).
    let numbering_marginal = numbering.as_ref().map(|numbering| {
        let both = match numbering {
            Numbering::Pattern(pattern) => pattern.pieces() >= 2,
            Numbering::Func(_) => true,
        };

        let mut counter = CounterDisplayElem::new(
            Counter::new(CounterKey::Page),
            Smart::Custom(numbering.clone()),
            both,
        )
        .pack();

        // We interpret the Y alignment as selecting header or footer
        // and then ignore it for aligning the actual number.
        if let Some(x) = number_align.x() {
            counter = counter.aligned(x.into());
        }

        counter
    });

    let header = styles.get_ref(PageElem::header);
    let footer = styles.get_ref(PageElem::footer);
    let (header, footer) = if matches!(number_align.y(), Some(OuterVAlignment::Top)) {
        (header.as_ref().unwrap_or(&numbering_marginal), footer.as_ref().unwrap_or(&None))
    } else {
        (header.as_ref().unwrap_or(&None), footer.as_ref().unwrap_or(&numbering_marginal))
    };

    // Layout the children.
    let area = size - margin.sum_by_axis();
    let fragment = layout_flow(
        &mut engine,
        children,
        &mut locator,
        styles,
        Regions::repeat(area, area.map(Abs::is_finite)),
        styles.get(PageElem::columns),
        styles.get(ColumnsElem::gutter).resolve(styles),
        FlowMode::Root,
    )?;

    // Layouts a single marginal.
    let mut layout_marginal = |content: &Option<Content>, area, align| {
        let Some(content) = content else { return Ok(None) };
        let aligned = content.clone().set(AlignElem::alignment, align);
        crate::layout_frame(
            &mut engine,
            &aligned,
            locator.next(&content.span()),
            styles,
            Region::new(area, Axes::splat(true)),
        )
        .map(Some)
    };

    // Layout marginals.
    let mut layouted = Vec::with_capacity(fragment.len());

    let header = header.clone().map(|h| h.artifact(ArtifactKind::Header));
    let footer = footer.clone().map(|f| f.artifact(ArtifactKind::Footer));
    let background = background.clone().map(|b| b.artifact(ArtifactKind::Page));

    for inner in fragment {
        let header_size = Size::new(inner.width(), margin.top - header_ascent);
        let footer_size = Size::new(inner.width(), margin.bottom - footer_descent);
        let full_size = inner.size() + margin.sum_by_axis();
        let mid = HAlignment::Center + VAlignment::Horizon;
        layouted.push(LayoutedPage {
            inner,
            fill: fill.clone(),
            numbering: numbering.clone(),
            supplement: supplement.clone(),
            header: layout_marginal(&header, header_size, Alignment::BOTTOM)?,
            footer: layout_marginal(&footer, footer_size, Alignment::TOP)?,
            background: layout_marginal(&background, full_size, mid)?,
            foreground: layout_marginal(foreground, full_size, mid)?,
            margin,
            binding,
            two_sided,
        });
    }

    Ok(layouted)
}

/// Result of preparing a page run for streaming processing.
/// Contains the flow Fragment and pre-computed page styles, so
/// LayoutedPages can be created per-frame without re-computing styles.
pub struct PreparedPageRun {
    fragment: Option<Fragment>,
    pub fill: Smart<Option<Paint>>,
    pub numbering: Option<Numbering>,
    pub supplement: Content,
    pub header: Option<Content>,
    pub footer: Option<Content>,
    pub background: Option<Content>,
    pub foreground: Option<Content>,
    pub margin: Sides<Abs>,
    pub header_ascent: Abs,
    pub footer_descent: Abs,
    pub binding: Binding,
    pub two_sided: bool,
    /// Root styles for the page run, used for marginal layout.
    pub styles: Styles,
}

impl PreparedPageRun {
    /// Take the fragment out for iteration while keeping the styles alive.
    pub fn take_fragment(&mut self) -> Fragment {
        self.fragment.take().expect("fragment already taken")
    }
}

/// Prepare a page run for streaming processing. Performs the flow layout
/// and returns the Fragment along with page styles, without creating
/// LayoutedPages. The caller can iterate the Fragment lazily and
/// create LayoutedPages one at a time using `create_layouted_page`.
pub fn prepare_page_run(
    engine: &mut Engine,
    children: &[Pair],
    locator: Locator,
    initial: StyleChain,
) -> SourceResult<PreparedPageRun> {
    let mut locator = locator.split();

    // Determine the page-wide styles (same as layout_page_run_impl).
    // Keep root_styles owned so we can move it into PreparedPageRun.
    let root_styles = Styles::root(children, initial);
    let styles = StyleChain::new(&root_styles);

    let width = styles.resolve(PageElem::width).unwrap_or(Abs::inf());
    let height = styles.resolve(PageElem::height).unwrap_or(Abs::inf());
    let mut size = Size::new(width, height);
    if styles.get(PageElem::flipped) {
        std::mem::swap(&mut size.x, &mut size.y);
    }

    let mut min = width.min(height);
    if !min.is_finite() {
        min = Paper::A4.width();
    }

    let default = Rel::<Length>::from((2.5 / 21.0) * min);
    let margin = styles.get(PageElem::margin);
    let two_sided = margin.two_sided.unwrap_or(false);
    let margin = margin
        .sides
        .map(|side| side.and_then(Smart::custom).unwrap_or(default))
        .resolve(styles)
        .relative_to(size);

    let fill = styles.get_cloned(PageElem::fill);
    let foreground = styles.get_ref(PageElem::foreground);
    let background = styles.get_ref(PageElem::background);
    let header_ascent = styles.resolve(PageElem::header_ascent).relative_to(margin.top);
    let footer_descent =
        styles.resolve(PageElem::footer_descent).relative_to(margin.bottom);
    let numbering = styles.get_ref(PageElem::numbering);
    let supplement = match styles.get_cloned(PageElem::supplement) {
        Smart::Auto => TextElem::packed(PageElem::local_name_in(styles)),
        Smart::Custom(content) => content.unwrap_or_default(),
    };
    let number_align = styles.get(PageElem::number_align);
    let binding = styles.get(PageElem::binding).unwrap_or_else(|| {
        match styles.resolve(TextElem::dir) {
            Dir::LTR => Binding::Left,
            _ => Binding::Right,
        }
    });

    let numbering_marginal = numbering.as_ref().map(|numbering| {
        let both = match numbering {
            Numbering::Pattern(pattern) => pattern.pieces() >= 2,
            Numbering::Func(_) => true,
        };
        let mut counter = CounterDisplayElem::new(
            Counter::new(CounterKey::Page),
            Smart::Custom(numbering.clone()),
            both,
        )
        .pack();
        if let Some(x) = number_align.x() {
            counter = counter.aligned(x.into());
        }
        counter
    });

    let header = styles.get_ref(PageElem::header);
    let footer = styles.get_ref(PageElem::footer);
    let (header, footer) = if matches!(number_align.y(), Some(OuterVAlignment::Top)) {
        (header.as_ref().unwrap_or(&numbering_marginal), footer.as_ref().unwrap_or(&None))
    } else {
        (header.as_ref().unwrap_or(&None), footer.as_ref().unwrap_or(&numbering_marginal))
    };

    // Layout the flow.
    let area = size - margin.sum_by_axis();
    let fragment = layout_flow(
        engine,
        children,
        &mut locator,
        styles,
        Regions::repeat(area, area.map(Abs::is_finite)),
        styles.get(PageElem::columns),
        styles.get(ColumnsElem::gutter).resolve(styles),
        FlowMode::Root,
    )?;

    let header = header.clone().map(|h| h.artifact(ArtifactKind::Header));
    let footer = footer.clone().map(|f| f.artifact(ArtifactKind::Footer));
    let background = background.clone().map(|b| b.artifact(ArtifactKind::Page));

    Ok(PreparedPageRun {
        fragment: Some(fragment),
        fill,
        numbering: numbering.clone(),
        supplement,
        header,
        footer,
        foreground: foreground.clone(),
        background,
        margin,
        header_ascent,
        footer_descent,
        binding,
        two_sided,
        styles: root_styles,
    })
}

/// Create a LayoutedPage from a single frame and prepared page run config.
pub fn create_layouted_page(
    engine: &mut Engine,
    inner: Frame,
    run: &PreparedPageRun,
    locator: Locator,
) -> SourceResult<LayoutedPage> {
    let styles = StyleChain::new(&run.styles);
    let mut locator = locator.split();
    let header_size = Size::new(inner.width(), run.margin.top - run.header_ascent);
    let footer_size = Size::new(inner.width(), run.margin.bottom - run.footer_descent);
    let full_size = inner.size() + run.margin.sum_by_axis();
    let mid = HAlignment::Center + VAlignment::Horizon;

    let mut layout_marginal = |content: &Option<Content>, area, align| {
        let Some(content) = content else { return Ok(None) };
        let aligned = content.clone().set(AlignElem::alignment, align);
        crate::layout_frame(
            engine,
            &aligned,
            locator.next(&content.span()),
            styles,
            Region::new(area, Axes::splat(true)),
        )
        .map(Some)
    };

    Ok(LayoutedPage {
        inner,
        fill: run.fill.clone(),
        numbering: run.numbering.clone(),
        supplement: run.supplement.clone(),
        header: layout_marginal(&run.header, header_size, Alignment::BOTTOM)?,
        footer: layout_marginal(&run.footer, footer_size, Alignment::TOP)?,
        background: layout_marginal(&run.background, full_size, mid)?,
        foreground: layout_marginal(&run.foreground, full_size, mid)?,
        margin: run.margin,
        binding: run.binding,
        two_sided: run.two_sided,
    })
}
