use std::io::Write as _;
use std::num::NonZeroU32;
use std::ops::Range;
use std::sync::Arc;

use krilla::tagging as kt;
use krilla::tagging::{NaiveRgbColor, Tag, TagKind};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use ecow::EcoString;
use typst_library::foundations::{Packed, Smart};
use typst_library::layout::resolve::{GridMeta, Line, LinePosition};
use typst_library::layout::{Abs, Sides};
use typst_library::model::TableElem;
use typst_library::pdf::{TableCellKind, TableHeaderScope};
use typst_library::visualize::{FixedStroke, Stroke};

use crate::tags::GroupId;
use crate::tags::context::grid::{CtxCell, GridCells, GridEntry, GridExt};
use crate::tags::context::{TableId, TagId};
use crate::tags::groups::CellInfo;
use crate::tags::tree::Tree;
use crate::tags::util::{self, PropertyOptRef, TableHeaderScopeExt};
use crate::util::{AbsExt, SidesExt};

#[derive(Debug)]
pub struct TableCtx {
    pub group_id: GroupId,
    pub table_id: TableId,
    /// Grid metadata extracted from the table element. Stored separately
    /// so we can drop the heavy `Packed<TableElem>` reference early,
    /// freeing the Content tree (~912 MB for 100K-row tables).
    grid_meta: Arc<GridMeta>,
    /// Table summary for accessibility, extracted from the table element.
    summary: Option<EcoString>,
    row_kinds: Vec<TableCellKind>,
    cells: GridCells<TableCellData>,
    border_thickness: Option<f32>,
    border_color: Option<NaiveRgbColor>,
    border_style: Option<kt::BorderStyle>,
}

#[derive(Debug, Clone)]
pub struct TableCellData {
    tag: TagId,
    kind: TableCellKind,
    headers: SmallVec<[kt::TagId; 1]>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PrioritzedStroke {
    stroke: Option<Arc<Stroke<Abs>>>,
    priority: StrokePriority,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum StrokePriority {
    GridStroke = 0,
    CellStroke = 1,
    ExplicitLine = 2,
}

impl TableCtx {
    pub fn new(group_id: GroupId, table_id: TableId, table: Packed<TableElem>) -> Self {
        let grid_meta = table.grid_meta.as_ref().unwrap().clone();
        let summary = table.summary.opt_ref().cloned();
        let width = grid_meta.non_gutter_column_count();
        let height = grid_meta.non_gutter_row_count();

        // Generate the default row kinds.
        let mut grid_headers = grid_meta.headers.iter().peekable();
        let default_row_kinds = (0..height as u32)
            .map(|y| {
                let grid_y = grid_meta.to_effective(y);

                // Find current header
                while grid_headers.next_if(|h| h.range.end <= grid_y).is_some() {}
                if let Some(header) = grid_headers.peek()
                    && header.range.contains(&grid_y)
                {
                    return TableCellKind::Header(header.level, TableHeaderScope::Column);
                }

                if let Some(footer) = &grid_meta.footer
                    && footer.range().contains(&grid_y)
                {
                    return TableCellKind::Footer;
                }

                TableCellKind::Data
            })
            .collect::<Vec<_>>();

        // Drop the heavy Packed<TableElem> — we've extracted what we need.
        // This releases the reference to the table element Content, which
        // in turn holds references to all ~100K cell Content objects.
        drop(table);

        Self {
            group_id,
            table_id,
            grid_meta,
            summary,
            row_kinds: default_row_kinds,
            cells: GridCells::new(width, height),
            border_thickness: None,
            border_color: None,
            border_style: None,
        }
    }

    pub fn insert(&mut self, info: &CellInfo, tag: TagId, id: GroupId) {
        let x: u32 = info.x();
        let y: u32 = info.y();
        let rowspan = info.rowspan();
        let colspan = info.colspan();

        let kind = info.kind()
            .and_then(|k| match k { Smart::Custom(k) => Some(k), Smart::Auto => None })
            .unwrap_or(self.row_kinds[y as usize]);

        self.cells.insert(CtxCell {
            data: TableCellData { tag, kind, headers: SmallVec::new() },
            x,
            y,
            rowspan: NonZeroU32::new(rowspan).unwrap_or(NonZeroU32::MIN),
            colspan: NonZeroU32::new(colspan).unwrap_or(NonZeroU32::MIN),
            id,
        });
    }

    pub fn build_tag(&self) -> TagKind {
        Tag::Table
            .with_summary(self.summary.as_ref().map(Into::into))
            .with_border_thickness(self.border_thickness.map(kt::Sides::uniform))
            .with_border_color(self.border_color.map(kt::Sides::uniform))
            .with_border_style(self.border_style.map(kt::Sides::uniform))
            .into()
    }

    /// Free the heavy cells grid after `build_table` has extracted all
    /// needed data into Groups/TagStorage. Only `build_tag()` is needed
    /// after this, which uses summary/border fields.
    pub fn free_cells(&mut self) {
        self.cells.clear();
        self.row_kinds = Vec::new();
        // Drop the Arc<GridMeta> reference — no longer needed after build_table.
        // For 100K-row tables this frees ~40 MB if this was the last reference.
        self.grid_meta = Arc::new(GridMeta {
            entries: Vec::new(),
            unique_strokes: Vec::new(),
            content_cols: 0,
            content_rows: 0,
            has_gutter: false,
            headers: Vec::new(),
            footer: None,
            hlines: Vec::new(),
            vlines: Vec::new(),
        });
    }
}

pub fn build_table(tree: &mut Tree, table_id: TableId) {
    let table_ctx = tree.ctx.tables.get_mut(table_id);

    // Table layouting ensures that there are no overlapping cells, and that
    // any gaps left by the user are filled with empty cells.
    // A show rule, can prevent the table from being properly laid out, in which
    // case cells will be missing.
    if table_ctx.cells.is_empty() || table_ctx.cells.iter().any(GridEntry::is_missing) {
        // Insert all children, so the content is included in the tag tree,
        // otherwise krilla might panic.
        for cell in table_ctx.cells.iter().filter_map(GridEntry::as_cell) {
            tree.groups.push_group(table_ctx.group_id, cell.id);
        }

        return;
    }

    let width = table_ctx.cells.width();
    let height = table_ctx.cells.height();
    let grid = &*table_ctx.grid_meta;

    // Only generate row groups such as `THead`, `TFoot`, and `TBody` if
    // there are no rows with mixed cell kinds, and there is at least one
    // header or a footer.
    let gen_row_groups = {
        let mut uniform_rows = true;
        let mut has_header_or_footer = false;
        let mut has_body = false;
        'outer: for (row, row_kind) in
            table_ctx.cells.rows().zip(table_ctx.row_kinds.iter_mut())
        {
            let first_cell = table_ctx.cells.resolve(row.first().unwrap()).unwrap();
            let first_kind = first_cell.data.kind;

            for cell in row.iter().filter_map(|cell| table_ctx.cells.resolve(cell)) {
                if let TableCellKind::Header(_, scope) = cell.data.kind
                    && scope != TableHeaderScope::Column
                {
                    uniform_rows = false;
                    break 'outer;
                }

                if first_kind != cell.data.kind {
                    uniform_rows = false;
                    break 'outer;
                }
            }

            // If all cells in the row have the same custom kind, the row
            // kind is overwritten.
            *row_kind = first_kind;

            has_header_or_footer |= *row_kind != TableCellKind::Data;
            has_body |= *row_kind == TableCellKind::Data;
        }

        uniform_rows && has_header_or_footer && has_body
    };

    // Compute the headers attribute column-wise.
    for x in 0..width {
        let mut column_headers = Vec::new();
        let mut grid_headers = grid.headers.iter().peekable();
        for y in 0..height {
            // Find current header region
            let grid_y = grid.to_effective(y);
            while grid_headers.next_if(|h| h.range.end <= grid_y).is_some() {}
            let region_range = grid_headers.peek().and_then(|header| {
                if !header.range.contains(&grid_y) {
                    return None;
                }

                // Convert from the `CellGrid` coordinates to normal ones.
                let start = grid.from_effective(header.range.start);
                let end = grid.from_effective(header.range.end);
                Some(start..end)
            });

            resolve_cell_headers(
                table_ctx.table_id,
                &mut table_ctx.cells,
                &mut column_headers,
                region_range,
                TableHeaderScope::refers_to_column,
                (x, y),
            );
        }
    }
    // Compute the headers attribute row-wise.
    for y in 0..height {
        let mut row_headers = Vec::new();
        for x in 0..width {
            resolve_cell_headers(
                table_ctx.table_id,
                &mut table_ctx.cells,
                &mut row_headers,
                None,
                TableHeaderScope::refers_to_row,
                (x, y),
            );
        }
    }

    // Build stroke grid from GridMeta. Strokes are NOT stored per-cell in
    // TableCellData to keep GridEntry small (~64 bytes vs ~128 bytes), saving
    // ~68 MB for 100K-row tables. The stroke grid is temporary — alive only
    // during build_table.
    let mut stroke_grid = StrokeGrid::from_grid(grid, &table_ctx.cells, width, height);

    // Place h-lines, overwriting strokes.
    // h-lines: block_idx = y (row), inline_idx = x (column)
    place_explicit_lines_on_grid(
        &mut stroke_grid,
        &table_ctx.cells,
        &grid.hlines,
        height,
        width,
        |block, inline| (inline, block), // (y, x) → (x, y)
        |stroke, pos| match pos {
            LinePosition::Before => &mut stroke.bottom,
            LinePosition::After => &mut stroke.top,
        },
    );
    // Place v-lines, overwriting strokes.
    // v-lines: block_idx = x (column), inline_idx = y (row)
    place_explicit_lines_on_grid(
        &mut stroke_grid,
        &table_ctx.cells,
        &grid.vlines,
        width,
        height,
        |block, inline| (block, inline), // (x, y) → (x, y)
        |stroke, pos| match pos {
            LinePosition::Before => &mut stroke.right,
            LinePosition::After => &mut stroke.left,
        },
    );

    // Remove overlapping border strokes between cells.
    for y in 0..height {
        for x in 0..width.saturating_sub(1) {
            prioritize_grid_strokes(
                &mut stroke_grid,
                &table_ctx.cells,
                (x, y),
                (x + 1, y),
                |a, b| (&mut a.right, &mut b.left),
            );
        }
    }
    for x in 0..width {
        for y in 0..height.saturating_sub(1) {
            prioritize_grid_strokes(
                &mut stroke_grid,
                &table_ctx.cells,
                (x, y),
                (x, y + 1),
                |a, b| (&mut a.bottom, &mut b.top),
            );
        }
    }

    (table_ctx.border_thickness, table_ctx.border_color, table_ctx.border_style) =
        try_resolve_table_stroke_from_grid(&stroke_grid, &table_ctx.cells);

    let mut chunk_kind = table_ctx.row_kinds[0];
    let mut chunk_id = GroupId::INVALID;
    for (row, y) in table_ctx.cells.rows_mut().zip(0..) {
        let parent = if gen_row_groups {
            let row_kind = table_ctx.row_kinds[y as usize];
            let is_first = chunk_id == GroupId::INVALID;
            if is_first || !should_group_rows(chunk_kind, row_kind) {
                let tag: TagKind = match row_kind {
                    // Only one `THead` group at the start of the table is permitted.
                    TableCellKind::Header(..) if is_first => Tag::THead.into(),
                    TableCellKind::Header(..) => Tag::TBody.into(),
                    TableCellKind::Footer => Tag::TFoot.into(),
                    TableCellKind::Data => Tag::TBody.into(),
                };
                chunk_kind = row_kind;
                chunk_id = tree.groups.push_tag(table_ctx.group_id, tag);
            }
            chunk_id
        } else {
            table_ctx.group_id
        };

        let row_id = tree.groups.push_tag(parent, Tag::TR);
        let row_nodes = row
            .iter_mut()
            .filter_map(|entry| {
                let cell = entry.as_cell_mut()?;
                let rowspan = (cell.rowspan.get() != 1).then_some(cell.rowspan);
                let colspan = (cell.colspan.get() != 1).then_some(cell.colspan);
                let cell_kind = cell.data.kind;
                let headers = std::mem::take(&mut cell.data.headers);
                let mut tag: TagKind = match cell_kind {
                    TableCellKind::Header(_, scope) => {
                        let id = table_cell_id(table_ctx.table_id, cell.x, cell.y);
                        Tag::TH(scope.to_krilla())
                            .with_id(Some(id))
                            .with_headers(Some(headers))
                            .with_row_span(rowspan)
                            .with_col_span(colspan)
                            .into()
                    }
                    TableCellKind::Footer | TableCellKind::Data => Tag::TD
                        .with_headers(Some(headers))
                        .with_row_span(rowspan)
                        .with_col_span(colspan)
                        .into(),
                };

                let cell_stroke = stroke_grid.get(cell.x, cell.y);
                resolve_cell_border_and_background(
                    grid,
                    table_ctx.border_thickness,
                    table_ctx.border_color,
                    table_ctx.border_style,
                    [cell.x, cell.y],
                    cell_stroke,
                    &mut tag,
                );

                tree.groups.tags.set(cell.data.tag, tag);

                Some(cell.id)
            })
            .collect::<Vec<_>>();

        tree.groups.push_groups(row_id, &row_nodes);
    }
}

fn should_group_rows(a: TableCellKind, b: TableCellKind) -> bool {
    match (a, b) {
        (TableCellKind::Header(..), TableCellKind::Header(..)) => true,
        (TableCellKind::Footer, TableCellKind::Footer) => true,
        (TableCellKind::Data, TableCellKind::Data) => true,
        (_, _) => false,
    }
}

struct HeaderCells {
    /// If this header is inside a table header regions defined by a
    /// `table.header()` call, this is the range of that region.
    /// Currently this is only supported for multi row headers.
    region_range: Option<Range<u32>>,
    level: NonZeroU32,
    cell_ids: SmallVec<[kt::TagId; 1]>,
}

fn resolve_cell_headers<F>(
    table_id: TableId,
    cells: &mut GridCells<TableCellData>,
    header_stack: &mut Vec<HeaderCells>,
    region_range: Option<Range<u32>>,
    refers_to_dir: F,
    (x, y): (u32, u32),
) where
    F: Fn(&TableHeaderScope) -> bool,
{
    let Some(cell) = cells.cell_mut(x, y) else { return };

    let cell_ids = resolve_cell_header_ids(
        table_id,
        header_stack,
        region_range,
        refers_to_dir,
        cell,
    );

    if let Some(header) = cell_ids {
        for id in header.cell_ids.iter() {
            if !cell.data.headers.contains(id) {
                cell.data.headers.push(id.clone());
            }
        }
    }
}

fn resolve_cell_header_ids<'a, F>(
    table_id: TableId,
    header_stack: &'a mut Vec<HeaderCells>,
    region_range: Option<Range<u32>>,
    refers_to_dir: F,
    cell: &CtxCell<TableCellData>,
) -> Option<&'a HeaderCells>
where
    F: Fn(&TableHeaderScope) -> bool,
{
    let TableCellKind::Header(level, scope) = cell.data.kind else {
        return header_stack.last();
    };
    if !refers_to_dir(&scope) {
        return header_stack.last();
    }

    // Remove all headers with a higher level.
    while header_stack.pop_if(|h| h.level > level).is_some() {}

    let tag_id = table_cell_id(table_id, cell.x, cell.y);

    // Check for multi-row header regions with the same level.
    let Some(prev) = header_stack.last_mut().filter(|h| h.level == level) else {
        header_stack.push(HeaderCells {
            region_range,
            level,
            cell_ids: SmallVec::from_buf([tag_id]),
        });
        return header_stack.iter().rev().nth(1);
    };

    // If the current header region encompasses the cell, add the cell id to
    // the header. This way multiple consecutive header cells in a single header
    // region will be listed for the next cells.
    if prev.region_range.clone().is_some_and(|r| r.contains(&cell.y)) {
        prev.cell_ids.push(tag_id);
    } else {
        // The current region doesn't encompass the cell.
        // Replace the previous heading that had the same level.
        *prev = HeaderCells {
            region_range,
            level,
            cell_ids: SmallVec::from_buf([tag_id]),
        };
    }

    header_stack.iter().rev().nth(1)
}

fn table_cell_id(table_id: TableId, x: u32, y: u32) -> kt::TagId {
    // 32 bytes is the maximum length the ID string can have.
    let mut buf = SmallVec::<[u8; 32]>::new();
    _ = write!(&mut buf, "{}x{x}y{y}", table_id.get() + 1);
    kt::TagId::from(buf)
}

/// Temporary stroke grid built from GridMeta at the start of build_table.
/// Keeps strokes out of TableCellData, reducing GridEntry from ~128 to ~64
/// bytes and saving ~68 MB for 100K-row tables.
struct StrokeGrid {
    strokes: Vec<Sides<PrioritzedStroke>>,
    width: usize,
}

impl StrokeGrid {
    /// Build the stroke grid from GridMeta for all cells in the table.
    fn from_grid(
        grid: &GridMeta,
        cells: &GridCells<TableCellData>,
        width: u32,
        height: u32,
    ) -> Self {
        let w = width as usize;
        let default = Sides::splat(PrioritzedStroke {
            stroke: None,
            priority: StrokePriority::GridStroke,
        });
        let mut strokes = vec![default; w * height as usize];

        for cell in cells.iter().filter_map(GridEntry::as_cell) {
            let [grid_x, grid_y] = [cell.x, cell.y].map(|i| grid.to_effective(i));
            if let Some(grid_cell) = grid.cell(grid_x, grid_y) {
                let pattern = grid.stroke_pattern(grid_cell);
                let stroke = pattern.stroke.clone().zip(pattern.stroke_overridden).map(
                    |(stroke, overridden)| {
                        let priority = if overridden {
                            StrokePriority::CellStroke
                        } else {
                            StrokePriority::GridStroke
                        };
                        PrioritzedStroke { stroke, priority }
                    },
                );
                strokes[cell.y as usize * w + cell.x as usize] = stroke;
            }
        }

        Self { strokes, width: w }
    }

    fn get(&self, x: u32, y: u32) -> &Sides<PrioritzedStroke> {
        &self.strokes[y as usize * self.width + x as usize]
    }

    fn get_mut(&mut self, x: u32, y: u32) -> &mut Sides<PrioritzedStroke> {
        &mut self.strokes[y as usize * self.width + x as usize]
    }

    /// Resolve a position through the cells grid (handling Spanned entries)
    /// and return the parent cell's stroke.
    fn resolve_mut<'a>(
        &'a mut self,
        cells: &GridCells<TableCellData>,
        x: u32,
        y: u32,
    ) -> Option<&'a mut Sides<PrioritzedStroke>> {
        let cell = cells.resolve_at(x, y)?;
        Some(self.get_mut(cell.x, cell.y))
    }
}

fn place_explicit_lines_on_grid<F, G>(
    stroke_grid: &mut StrokeGrid,
    cells: &GridCells<TableCellData>,
    lines: &[Vec<Line>],
    block_end: u32,
    inline_end: u32,
    to_xy: G,
    get_side: F,
) where
    F: Fn(&mut Sides<PrioritzedStroke>, LinePosition) -> &mut PrioritzedStroke,
    G: Fn(u32, u32) -> (u32, u32),
{
    for line in lines.iter().flat_map(|lines| lines.iter()) {
        let end = line.end.map(|n| n.get() as u32).unwrap_or(inline_end).min(inline_end);
        let explicit_stroke = || PrioritzedStroke {
            stroke: line.stroke.clone(),
            priority: StrokePriority::ExplicitLine,
        };

        // Fixup line positions before the first, or after the last cell.
        let mut pos = line.position;
        if line.index == 0 {
            pos = LinePosition::After;
        } else if line.index + 1 == block_end as usize {
            pos = LinePosition::Before;
        };

        let block_idx = match pos {
            LinePosition::Before => (line.index - 1) as u32,
            LinePosition::After => line.index as u32,
        };
        for inline_idx in line.start as u32..end {
            let (x, y) = to_xy(block_idx, inline_idx);
            if let Some(cell_stroke) = stroke_grid.resolve_mut(cells, x, y) {
                let side = get_side(cell_stroke, pos);
                *side = explicit_stroke();
            }
        }
    }
}

/// PDF tables don't support gutters, remove all overlapping strokes,
/// that aren't equal. Leave strokes that would overlap but are the same
/// because then only a single value has to be written for `BorderStyle`,
/// `BorderThickness`, and `BorderColor` instead of an array for each.
fn prioritize_grid_strokes<F>(
    stroke_grid: &mut StrokeGrid,
    cells: &GridCells<TableCellData>,
    a: (u32, u32),
    b: (u32, u32),
    get_sides: F,
) where
    F: for<'a> Fn(
        &'a mut Sides<PrioritzedStroke>,
        &'a mut Sides<PrioritzedStroke>,
    ) -> (&'a mut PrioritzedStroke, &'a mut PrioritzedStroke),
{
    // Resolve both positions to parent cells.
    let Some(cell_a) = cells.resolve_at(a.0, a.1) else { return };
    let Some(cell_b) = cells.resolve_at(b.0, b.1) else { return };

    let idx_a = cell_a.y as usize * stroke_grid.width + cell_a.x as usize;
    let idx_b = cell_b.y as usize * stroke_grid.width + cell_b.x as usize;

    if idx_a == idx_b {
        return; // Same parent cell (spanned), no conflict.
    }

    // Borrow disjoint entries from the strokes Vec.
    let (sa, sb) = if idx_a < idx_b {
        let (left, right) = stroke_grid.strokes.split_at_mut(idx_b);
        (&mut left[idx_a], &mut right[0])
    } else {
        let (left, right) = stroke_grid.strokes.split_at_mut(idx_a);
        (&mut right[0], &mut left[idx_b])
    };

    let (a_side, b_side) = get_sides(sa, sb);

    // Only remove contesting (different) edge strokes.
    if a_side.stroke != b_side.stroke {
        // Prefer the right stroke on same priorities.
        if a_side.priority <= b_side.priority {
            a_side.stroke = b_side.stroke.clone();
        } else {
            b_side.stroke = a_side.stroke.clone();
        }
    }
}

/// Try to resolve a table border stroke color and thickness that is inherited
/// by the cells. In Acrobat cells cannot override the border thickness or color
/// of the outer border around the table if the thickness is set.
fn try_resolve_table_stroke_from_grid(
    stroke_grid: &StrokeGrid,
    cells: &GridCells<TableCellData>,
) -> (Option<f32>, Option<NaiveRgbColor>, Option<kt::BorderStyle>) {
    // Omitted strokes are counted too for reasons explained above.
    let mut strokes = FxHashMap::<_, usize>::default();
    for cell in cells.iter().filter_map(GridEntry::as_cell) {
        let cell_stroke = stroke_grid.get(cell.x, cell.y);
        for stroke in cell_stroke.iter() {
            *strokes.entry(stroke.stroke.as_ref()).or_default() += 1;
        }
    }

    let uniform_stroke = strokes.len() == 1;

    // Find the most used stroke and convert it to a fixed stroke.
    let stroke = strokes.into_iter().max_by_key(|(_, num)| *num).and_then(|(s, _)| {
        let s = (**s?).clone();
        Some(s.unwrap_or_default())
    });
    let Some(stroke) = stroke else { return (None, None, None) };

    // Only set parent stroke attributes if the table uses one uniform stroke.
    let thickness = uniform_stroke.then_some(stroke.thickness.to_f32());
    let style = uniform_stroke.then_some(match stroke.dash {
        Some(_) => kt::BorderStyle::Dashed,
        None => kt::BorderStyle::Solid,
    });
    let color = util::paint_to_color(&stroke.paint);

    (thickness, color, style)
}

fn resolve_cell_border_and_background(
    grid: &GridMeta,
    parent_border_thickness: Option<f32>,
    parent_border_color: Option<NaiveRgbColor>,
    parent_border_style: Option<kt::BorderStyle>,
    pos: [u32; 2],
    stroke: &Sides<PrioritzedStroke>,
    tag: &mut TagKind,
) {
    // Resolve border attributes.
    let fixed = stroke
        .as_ref()
        .map(|s| s.stroke.as_ref().map(|s| (**s).clone().unwrap_or_default()));

    // Acrobat completely ignores the border style attribute, but the spec
    // defines `BorderStyle::None` as the default. So make sure to write
    // the correct border styles. When a parent border_style is set (uniform
    // tables), cells that match inherit it → no per-cell attribute needed.
    let border_style = resolve_sides(&fixed, parent_border_style, Some(kt::BorderStyle::None), |s| {
        s.map(|s| match s.dash {
            Some(_) => kt::BorderStyle::Dashed,
            None => kt::BorderStyle::Solid,
        })
    });

    // In Acrobat `BorderThickness` takes precedence over `BorderStyle`. If
    // A `BorderThickness != 0` is specified for a side the border is drawn
    // even if `BorderStyle::None` is set. So explicitly write zeros for
    // sides that should be omitted.
    let border_thickness =
        resolve_sides(&fixed, parent_border_thickness, Some(0.0), |s| {
            s.map(|s| s.thickness.to_f32())
        });

    let border_color = resolve_sides(&fixed, parent_border_color, None, |s| {
        s.and_then(|s| util::paint_to_color(&s.paint))
    });

    tag.set_border_style(border_style);
    tag.set_border_thickness(border_thickness);
    tag.set_border_color(border_color);

    let [grid_x, grid_y] = pos.map(|i| grid.to_effective(i));
    let grid_cell = grid.cell(grid_x, grid_y).unwrap();
    let background_color =
        grid_cell.fill_rgb.map(|[r, g, b]| NaiveRgbColor::new(r, g, b));
    tag.set_background_color(background_color);
}

/// Try to minimize the attributes written per cell.
/// The parent value will be set on the table tag and is inherited by all table
/// cells. If all present values match the parent or all are missing, the
/// attribute can be omitted, and thus `None` is returned.
/// If one of the present values differs from the parent value, the cell
/// attribute needs to override the parent attribute, fill up the remaining
/// sides with a `default` value if provided, or any other present value.
///
/// Using an already present value has the benefit of saving storage space in
/// the resulting PDF, if all sides have the same value, because then a
/// [kt::Sides::uniform] value can be written instead of an 4-element array.
fn resolve_sides<F, T>(
    sides: &Sides<Option<FixedStroke>>,
    parent: Option<T>,
    default: Option<T>,
    map: F,
) -> Option<kt::Sides<T>>
where
    T: Copy + PartialEq,
    F: Copy + Fn(Option<&FixedStroke>) -> Option<T>,
{
    let mapped = sides.as_ref().map(|s| map(s.as_ref()));

    if mapped.iter().flatten().all(|v| Some(*v) == parent) {
        // All present values are equal to the parent value.
        return None;
    }

    let Some(first) = mapped.iter().flatten().next() else {
        // All values are missing
        return None;
    };

    // At least one value is different from the parent, fill up the remaining
    // sides with a replacement value.
    let replacement = default.unwrap_or(*first);
    let sides = mapped.unwrap_or(replacement);

    // TODO(accessibility): handle `text(dir: rtl)`
    Some(sides.to_lrtb_krilla())
}
