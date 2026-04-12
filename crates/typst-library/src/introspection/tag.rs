use std::fmt::{self, Debug, Formatter};
use std::num::NonZeroU32;

use crate::diag::{SourceResult, bail};
use crate::engine::Engine;
use crate::foundations::{
    Args, Construct, Content, NativeElement, Packed, Smart, Unlabellable, elem,
};
use crate::introspection::Location;
use crate::pdf::{TableCellKind, TableHeaderScope};

/// Marks the start or end of a locatable element.
#[derive(Clone, PartialEq, Hash)]
pub enum Tag {
    /// The stored element starts here.
    ///
    /// The [`Location`] is stored directly on the tag so that creators
    /// (e.g. grid cell tag generation) do not need to call
    /// `set_location` on the [`Content`], which would trigger a deep
    /// clone via `Arc::make_mut`.
    Start(Content, Location, TagFlags),
    /// The element with the given location and key hash ends here.
    ///
    /// Note: The key hash is stored here instead of in `Start` simply to make
    /// the two enum variants more balanced in size, keeping a `Tag`'s memory
    /// size down. There are no semantic reasons for this.
    End(Location, u128, TagFlags),
    /// A compact cell tag that avoids allocating Packed<TableCell/GridCell>.
    /// Stores cell metadata inline (~16 bytes) instead of a full Content
    /// (~400 bytes per Packed).
    CellStart(CellTagMeta, Location, TagFlags),
}

impl Tag {
    /// Access the location of the tag.
    pub fn location(&self) -> Location {
        match self {
            Tag::Start(_, loc, ..) => *loc,
            Tag::End(loc, ..) => *loc,
            Tag::CellStart(_, loc, ..) => *loc,
        }
    }
}

impl Debug for Tag {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let loc = self.location();
        match self {
            Tag::Start(elem, ..) => write!(f, "Start({:?}, {loc:?})", elem.elem().name()),
            Tag::End(..) => write!(f, "End({loc:?})"),
            Tag::CellStart(meta, ..) => write!(f, "CellStart({meta:?}, {loc:?})"),
        }
    }
}

/// Compact metadata for grid/table cell tags.
///
/// Avoids allocating a full Packed<TableCell/GridCell> per cell during
/// layout. At 16 bytes vs ~400 bytes per cell, this saves ~38 MB for
/// a 100K-cell table.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct CellTagMeta {
    /// Cell X position in the grid.
    pub x: u16,
    /// Cell Y position in the grid.
    pub y: u32,
    /// Number of columns this cell spans.
    pub colspan: u16,
    /// Number of rows this cell spans.
    pub rowspan: u16,
    /// Cell type and kind.
    pub kind: CellTagKind,
}

/// The kind of a cell tag.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum CellTagKind {
    /// A grid cell (non-table).
    GridCell,
    /// A table data cell.
    TableData,
    /// A table header cell.
    TableHeader { level: u8, scope: u8 },
    /// A table footer cell.
    TableFooter,
    /// A repeated cell (header/footer repeat, becomes artifact).
    Repeated,
}

impl CellTagMeta {
    /// Create metadata for a table cell.
    pub fn table(
        x: usize,
        y: usize,
        colspan: std::num::NonZeroUsize,
        rowspan: std::num::NonZeroUsize,
        kind: Smart<TableCellKind>,
        is_repeated: bool,
    ) -> Self {
        let cell_kind = if is_repeated {
            CellTagKind::Repeated
        } else {
            match kind {
                Smart::Custom(TableCellKind::Header(level, scope)) => {
                    CellTagKind::TableHeader {
                        level: level.get().min(255) as u8,
                        scope: scope as u8,
                    }
                }
                Smart::Custom(TableCellKind::Footer) => CellTagKind::TableFooter,
                Smart::Custom(TableCellKind::Data) | Smart::Auto => {
                    CellTagKind::TableData
                }
            }
        };
        Self {
            x: x.min(u16::MAX as usize) as u16,
            y: y.min(u32::MAX as usize) as u32,
            colspan: colspan.get().min(u16::MAX as usize) as u16,
            rowspan: rowspan.get().min(u16::MAX as usize) as u16,
            kind: cell_kind,
        }
    }

    /// Create metadata for a grid cell.
    pub fn grid(
        x: usize,
        y: usize,
        colspan: std::num::NonZeroUsize,
        rowspan: std::num::NonZeroUsize,
        is_repeated: bool,
    ) -> Self {
        Self {
            x: x.min(u16::MAX as usize) as u16,
            y: y.min(u32::MAX as usize) as u32,
            colspan: colspan.get().min(u16::MAX as usize) as u16,
            rowspan: rowspan.get().min(u16::MAX as usize) as u16,
            kind: if is_repeated { CellTagKind::Repeated } else { CellTagKind::GridCell },
        }
    }

    /// Whether this is a table cell (not a grid cell).
    pub fn is_table(&self) -> bool {
        !matches!(self.kind, CellTagKind::GridCell)
    }

    /// Convert to `Smart<TableCellKind>` for PDF tag builder compatibility.
    pub fn to_table_cell_kind(&self) -> Smart<TableCellKind> {
        match self.kind {
            CellTagKind::TableData => Smart::Auto,
            CellTagKind::TableHeader { level, scope } => {
                let level = NonZeroU32::new(level as u32).unwrap_or(NonZeroU32::MIN);
                let scope = match scope {
                    1 => TableHeaderScope::Column,
                    2 => TableHeaderScope::Row,
                    _ => TableHeaderScope::Both,
                };
                Smart::Custom(TableCellKind::Header(level, scope))
            }
            CellTagKind::TableFooter => Smart::Custom(TableCellKind::Footer),
            CellTagKind::GridCell | CellTagKind::Repeated => Smart::Auto,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TagFlags {
    /// Whether the element will be inserted into the
    /// [`Introspector`](super::Introspector).
    /// Either because it is [`Locatable`](super::Locatable), has been labelled,
    /// or a location has been manually set.
    pub introspectable: bool,
    /// Whether the element is [`Tagged`](super::Tagged).
    pub tagged: bool,
}

impl TagFlags {
    pub fn any(&self) -> bool {
        self.introspectable || self.tagged
    }
}

/// Holds a tag for a locatable element that was realized.
///
/// The `TagElem` is handled by all layouters. The held element becomes
/// available for introspection in the next compiler iteration.
#[elem(Construct, Unlabellable)]
pub struct TagElem {
    /// The introspectable element.
    #[required]
    #[internal]
    pub tag: Tag,
}

impl TagElem {
    /// Create a packed tag element.
    pub fn packed(tag: Tag) -> Content {
        let mut content = Self::new(tag).pack();
        // We can skip preparation for the `TagElem`.
        content.mark_prepared();
        content
    }
}

impl Construct for TagElem {
    fn construct(_: &mut Engine, args: &mut Args) -> SourceResult<Content> {
        bail!(args.span, "cannot be constructed manually")
    }
}

impl Unlabellable for Packed<TagElem> {}
