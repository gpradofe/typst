mod layouter;
mod lines;
mod repeated;
mod rowspans;

pub use self::layouter::{GridLayouter, reset_shared_output_store};

use std::sync::Arc;

use typst_library::diag::SourceResult;
use typst_library::engine::Engine;
use typst_library::foundations::{Packed, Smart, StyleChain};
use typst_library::introspection::{CellTagMeta, Location, Locator, SplitLocator, Tag, TagFlags};
use typst_library::layout::grid::resolve::{Cell, CellSource, cached_table_cellgrid, cached_grid_cellgrid, cellgrid_by_key};
use typst_library::layout::{
    Fragment, Frame, FrameItem, FrameParent, GridElem, Inherit,
    Point, Regions, Sides,
};
use typst_library::model::TableElem;

use self::layouter::RowPiece;
use self::lines::{
    LineSegment, generate_line_segments, hline_stroke_at_column, vline_stroke_at_row,
};
use self::rowspans::{Rowspan, UnbreakableRowGroup};


/// Layout the cell into the given regions.
///
/// The `disambiguator` indicates which instance of this cell this should be
/// layouted as. For normal cells, it is always `0`, but for headers and
/// footers, it indicates the index of the header/footer among all. See the
/// [`Locator`] docs for more details on the concepts behind this.
pub fn layout_cell(
    cell: &Cell,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: Regions,
    is_repeated: bool,
) -> SourceResult<Fragment> {
    // HACK: manually generate tags for table and grid cells. Ideally table and
    // grid cells could just be marked as locatable, but the tags are somehow
    // considered significant for layouting. This hack together with a check in
    // the grid layouter makes the test suite pass.
    let mut locator = locator.split();
    let mut tags = None;

    // Generate tags using compact CellTagMeta instead of allocating a full
    // Packed<TableCell/GridCell> per cell. CellTagMeta is ~16 bytes inline
    // vs ~400 bytes heap-allocated per Packed, saving ~38 MB for 100K cells.
    match &cell.source {
        Some(CellSource::Table { cell_x, cell_y, kind }) => {
            let meta = CellTagMeta::table(
                *cell_x, *cell_y, cell.colspan, cell.rowspan, *kind, is_repeated,
            );
            tags = Some(generate_cell_tags(meta, cell.source_span, &mut locator, engine));
        }
        Some(CellSource::Grid { cell_x, cell_y }) => {
            let meta = CellTagMeta::grid(
                *cell_x, *cell_y, cell.colspan, cell.rowspan, is_repeated,
            );
            tags = Some(generate_cell_tags(meta, cell.source_span, &mut locator, engine));
        }
        None => {}
    }

    let locator = locator.next(&cell.body.span());

    // When apply_inset_align is set, the cell body is raw content without
    // padded/aligned wrappers. Apply them on-the-fly here so they are
    // short-lived (freed after layout_fragment returns) instead of being
    // stored permanently in Cell.body for the entire document lifetime.
    let body;
    let layout_body = if cell.apply_inset_align {
        let mut b = cell.body.clone();
        let applied_inset = cell.resolved_inset
            .as_deref()
            .cloned()
            .unwrap_or_default()
            .map(Option::unwrap_or_default);
        if applied_inset != Sides::default() {
            b = b.padded(applied_inset);
        }
        if let Smart::Custom(alignment) = cell.resolved_align {
            b = b.aligned(alignment);
        }
        body = b;
        &body
    } else {
        &cell.body
    };

    let fragment = crate::layout_fragment(engine, layout_body, locator, styles, regions)?;

    Ok(apply_cell_tags(fragment.into_frames(), tags))
}

/// Apply cell tags to fragment frames.
fn apply_cell_tags(
    mut frames: Vec<Frame>,
    tags: Option<(Location, u128, Tag)>,
) -> Fragment {
    if let Some((loc, key, start_tag)) = tags
        && let Some((first, remainder)) = frames.split_first_mut()
    {
        let flags = TagFlags { introspectable: true, tagged: true };
        if remainder.is_empty() {
            if Arc::strong_count(first.items_arc()) > 1 {
                let size = first.size();
                let kind = first.kind();
                let original = std::mem::replace(first, Frame::new(size, kind));
                first.push(Point::zero(), FrameItem::Tag(start_tag));
                first.push(Point::zero(), FrameItem::Group(
                    typst_library::layout::GroupItem::new(original)
                ));
                first.push(Point::zero(), FrameItem::Tag(Tag::End(loc, key, flags)));
            } else {
                first.prepend(Point::zero(), FrameItem::Tag(start_tag));
                first.push(Point::zero(), FrameItem::Tag(Tag::End(loc, key, flags)));
            }
        } else {
            for frame in frames.iter_mut() {
                frame.set_parent(FrameParent::new(loc, Inherit::Yes));
            }
            frames.first_mut().unwrap().prepend_multiple([
                (Point::zero(), FrameItem::Tag(start_tag)),
                (Point::zero(), FrameItem::Tag(Tag::End(loc, key, flags))),
            ]);
        }
    }
    Fragment::frames(frames)
}

/// Generate compact cell tags without allocating Packed<TableCell/GridCell>.
fn generate_cell_tags(
    meta: CellTagMeta,
    span: typst_syntax::Span,
    locator: &mut SplitLocator,
    engine: &mut Engine,
) -> (Location, u128, Tag) {
    let key = typst_utils::hash128(&meta);
    let loc = locator.next_location(engine, key, span);
    let flags = TagFlags { introspectable: true, tagged: true };
    (loc, key, Tag::CellStart(meta, loc, flags))
}

/// Layout the grid.
#[typst_macros::time(span = elem.span())]
pub fn layout_grid(
    elem: &Packed<GridElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: Regions,
) -> SourceResult<Fragment> {
    // Always use the thread-local cache for CellGrid (MAX_CELLGRID_CACHE=30).
    // The grid is NOT stored on the element to avoid holding all grids in the
    // Content tree simultaneously (~42 MB for 218-table documents).
    let grid = match elem.grid.as_ref() {
        Some(g) => g.clone(),
        None => cached_grid_cellgrid(elem, engine, styles)?,
    };
    GridLayouter::new(&grid, regions, locator, styles, elem.span()).layout(engine)
}

/// Layout the table.
#[typst_macros::time(span = elem.span())]
pub fn layout_table(
    elem: &Packed<TableElem>,
    engine: &mut Engine,
    locator: Locator,
    styles: StyleChain,
    regions: Regions,
) -> SourceResult<Fragment> {
    // Use stored cache key from synthesize to find the CellGrid in cache.
    // This avoids recomputation caused by materialize() changing the element
    // hash between synthesize and layout_table.
    let grid = match elem.grid.as_ref() {
        Some(g) => g.clone(),
        None => match elem.grid_cache_key.as_ref() {
            Some(&key) => cellgrid_by_key(key, elem, engine, styles)?,
            None => cached_table_cellgrid(elem, engine, styles)?.0,
        },
    };
    GridLayouter::new(&grid, regions, locator, styles, elem.span()).layout(engine)
}
