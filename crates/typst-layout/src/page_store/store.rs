//! Disk-backed page store: serializes pages to a temp file and reads
//! them back one at a time during PDF export.

use std::io::{self, BufReader, BufWriter, Read, Write};

use typst_library::foundations::{Content, Smart};
use typst_library::introspection::Tag;
use typst_library::layout::{FrameItem, Point};
use typst_library::model::Numbering;

use super::converter::FrameConverter;
use super::types::*;
use crate::Page;

/// A disk-backed store for document pages.
///
/// Serializes page frames to a temporary file, keeping only lightweight
/// metadata (fonts, images, tags, numberings) in memory. Pages can be
/// read back one at a time for streaming export.
pub struct DiskPageStore {
    /// Temp file holding serialized page data.
    file: tempfile::NamedTempFile,
    /// Buffered writer for efficient appends (avoids per-page syscalls).
    writer: Option<BufWriter<std::fs::File>>,
    /// Running byte offset for the writer (tracked locally to avoid seek).
    write_offset: u64,
    /// Number of pages stored.
    page_count: usize,
    /// Byte offsets of each page in the file (for random access).
    offsets: Vec<u64>,
    /// Shared frame converter (holds fonts, images, tags, gradients, tilings).
    pub converter: FrameConverter,
    /// Numbering objects (contain Func, can't be serialized).
    numberings: Vec<Numbering>,
    /// Page supplement Content objects.
    supplements: Vec<Content>,
    /// Tags that belong at the end of the last page but arrived after
    /// the page was already serialized to disk.
    remaining_tags: Vec<Tag>,
}

impl DiskPageStore {
    /// Creates a new empty store backed by a temporary file.
    /// Pages can be appended one at a time via `append_page()`.
    pub fn new() -> io::Result<Self> {
        let file = tempfile::NamedTempFile::new()?;
        Ok(DiskPageStore {
            file,
            writer: None,
            write_offset: 0,
            page_count: 0,
            offsets: Vec::new(),
            converter: FrameConverter::new(),
            numberings: Vec::new(),
            supplements: Vec::new(),
            remaining_tags: Vec::new(),
        })
    }

    /// Creates a new store and serializes all pages to disk.
    /// After this call, the pages can be dropped from memory.
    pub fn from_pages(pages: &[Page]) -> io::Result<Self> {
        let file = tempfile::NamedTempFile::new()?;
        let mut writer = BufWriter::new(file.reopen()?);
        let mut store = DiskPageStore {
            file,
            writer: None,
            write_offset: 0,
            page_count: pages.len(),
            offsets: Vec::with_capacity(pages.len()),
            converter: FrameConverter::new(),
            numberings: Vec::new(),
            supplements: Vec::new(),
            remaining_tags: Vec::new(),
        };

        let mut offset: u64 = 0;
        for page in pages {
            store.offsets.push(offset);
            let spage = store.convert_page(page);
            let bytes = bincode::serialize(&spage).map_err(io::Error::other)?;
            let len = bytes.len() as u64;
            writer.write_all(&len.to_le_bytes())?;
            writer.write_all(&bytes)?;
            offset += 8 + len;
        }
        writer.flush()?;
        store.write_offset = offset;

        Ok(store)
    }

    /// Appends a single page to the store.
    pub fn append_page(&mut self, page: &Page) -> io::Result<()> {
        let spage = self.convert_page(page);
        let bytes = bincode::serialize(&spage).map_err(io::Error::other)?;

        // Lazily create the buffered writer on first append.
        if self.writer.is_none() {
            self.writer = Some(BufWriter::new(self.file.reopen()?));
        }
        let writer = self.writer.as_mut().unwrap();

        self.offsets.push(self.write_offset);
        let len = bytes.len() as u64;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&bytes)?;
        self.write_offset += 8 + len;

        self.page_count += 1;
        Ok(())
    }

    /// Flush the buffered writer. Must be called before any reads.
    pub fn flush_writer(&mut self) -> io::Result<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush()?;
        }
        // Drop the writer to release the file handle.
        self.writer = None;
        Ok(())
    }

    /// Returns the number of pages in the store.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Clears Content objects from the tag registry, freeing references
    /// to the Content tree. This saves ~912 MB for 100K-row table documents.
    /// Call after the PDF tag tree has been built from the stored pages.
    /// After this, reconstruct_tag returns CellStart with dummy metadata
    /// for Tag::Start items (only tag boundaries matter for PDF rendering).
    pub fn clear_tag_content(&mut self) {
        self.converter.clear_tags();
        // Also clear remaining_tags which may hold Tag::Start(Content, ..)
        self.remaining_tags.clear();
    }

    /// Sets remaining tags to be injected into the last page when read.
    pub fn set_remaining_tags(&mut self, tags: Vec<Tag>) {
        self.remaining_tags = tags;
    }

    /// Reads a single page back from disk and reconstructs it.
    pub fn read_page(&self, index: usize) -> io::Result<Page> {
        if index >= self.page_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "page index out of range",
            ));
        }

        let mut reader = BufReader::new(self.file.reopen()?);
        let offset = self.offsets[index];

        // Seek to the page's offset
        io::copy(&mut reader.by_ref().take(offset), &mut io::sink())?;

        // Read length prefix
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let len = u64::from_le_bytes(len_bytes) as usize;

        // Read serialized page
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;

        let spage: SPage = bincode::deserialize(&buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut page = self.reconstruct_page(spage);

        // Inject remaining tags into the last page.
        if index == self.page_count - 1 && !self.remaining_tags.is_empty() {
            let pos = Point::with_y(page.frame.height());
            page.frame.push_multiple(
                self.remaining_tags
                    .iter()
                    .cloned()
                    .map(|tag| (pos, FrameItem::Tag(tag))),
            );
        }

        Ok(page)
    }

    /// Returns an iterator that reads pages sequentially from disk.
    /// Uses a single buffered reader for efficient sequential access.
    pub fn pages_iter(&self) -> io::Result<SequentialPageIterator<'_>> {
        let reader = io::BufReader::new(self.file.reopen()?);
        Ok(SequentialPageIterator { store: self, reader, index: 0 })
    }

    /// Opens a sequential reader for raw page data.
    /// The returned reader is an independent file handle that does not
    /// borrow the store, allowing `&mut self` methods to be called
    /// between reads.
    pub fn open_sequential_reader(&self) -> io::Result<io::BufReader<std::fs::File>> {
        Ok(io::BufReader::new(self.file.reopen()?))
    }

    /// Reads the next page from a sequential reader using consuming tag
    /// reconstruction. Each tag's Content is taken out of the converter
    /// via `Option::take()`, freeing the reference immediately instead
    /// of keeping all Content alive for the entire page loop.
    ///
    /// `is_last` should be true for the last page so remaining tags are
    /// injected.
    pub fn read_next_page_consuming(
        &mut self,
        reader: &mut io::BufReader<std::fs::File>,
        is_last: bool,
    ) -> io::Result<Page> {
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let len = u64::from_le_bytes(len_bytes) as usize;

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;

        let spage: SPage = bincode::deserialize(&buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut page = self.reconstruct_page_consuming(spage);

        // Inject remaining tags into the last page.
        if is_last && !self.remaining_tags.is_empty() {
            let pos = Point::with_y(page.frame.height());
            page.frame.push_multiple(
                self.remaining_tags.drain(..).map(|tag| (pos, FrameItem::Tag(tag))),
            );
        }

        Ok(page)
    }

    /// Reconstructs a page using consuming tag reconstruction.
    fn reconstruct_page_consuming(&mut self, spage: SPage) -> Page {
        let frame = self.converter.reconstruct_frame_consuming(spage.frame);

        let fill = match spage.fill {
            None => Smart::Auto,
            Some(None) => Smart::Custom(None),
            Some(Some(paint)) => {
                Smart::Custom(Some(self.converter.reconstruct_paint(paint)))
            }
        };

        let numbering =
            spage.numbering_ref.map(|id| self.numberings[id as usize].clone());

        let supplement = self.supplements[spage.supplement_ref as usize].clone();

        Page {
            frame,
            fill,
            numbering,
            supplement,
            number: spage.number,
        }
    }

    // --- Conversion: Page → SPage (delegates frame conversion to FrameConverter) ---

    fn convert_page(&mut self, page: &Page) -> SPage {
        let frame = self.converter.convert_frame(&page.frame);

        let fill = match &page.fill {
            Smart::Auto => None,
            Smart::Custom(None) => Some(None),
            Smart::Custom(Some(paint)) => Some(Some(self.converter.convert_paint(paint))),
        };

        let numbering_ref = page.numbering.as_ref().map(|n| {
            let id = self.numberings.len() as u32;
            self.numberings.push(n.clone());
            id
        });

        let supplement_ref = self.supplements.len() as u32;
        self.supplements.push(page.supplement.clone());

        SPage {
            frame,
            fill,
            numbering_ref,
            supplement_ref,
            number: page.number,
        }
    }

    // --- Reconstruction: SPage → Page (delegates frame reconstruction to FrameConverter) ---

    fn reconstruct_page(&self, spage: SPage) -> Page {
        let frame = self.converter.reconstruct_frame(spage.frame);

        let fill = match spage.fill {
            None => Smart::Auto,
            Some(None) => Smart::Custom(None),
            Some(Some(paint)) => {
                Smart::Custom(Some(self.converter.reconstruct_paint(paint)))
            }
        };

        let numbering =
            spage.numbering_ref.map(|id| self.numberings[id as usize].clone());

        let supplement = self.supplements[spage.supplement_ref as usize].clone();

        Page {
            frame,
            fill,
            numbering,
            supplement,
            number: spage.number,
        }
    }
}

/// Sequential page iterator using a single buffered reader.
/// Much faster than random-access `read_page` for sequential reads.
pub struct SequentialPageIterator<'a> {
    store: &'a DiskPageStore,
    reader: io::BufReader<std::fs::File>,
    index: usize,
}

impl Iterator for SequentialPageIterator<'_> {
    type Item = io::Result<Page>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.store.page_count {
            return None;
        }

        let is_last = self.index == self.store.page_count - 1;
        let result = (|| -> io::Result<Page> {
            let mut len_bytes = [0u8; 8];
            self.reader.read_exact(&mut len_bytes)?;
            let len = u64::from_le_bytes(len_bytes) as usize;

            let mut buf = vec![0u8; len];
            self.reader.read_exact(&mut buf)?;

            let spage: SPage = bincode::deserialize(&buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            let mut page = self.store.reconstruct_page(spage);

            // Inject remaining tags into the last page.
            if is_last && !self.store.remaining_tags.is_empty() {
                let pos = Point::with_y(page.frame.height());
                page.frame.push_multiple(
                    self.store
                        .remaining_tags
                        .iter()
                        .cloned()
                        .map(|tag| (pos, FrameItem::Tag(tag))),
                );
            }

            Ok(page)
        })();

        self.index += 1;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use typst_library::foundations::{Content, Smart};
    use typst_library::layout::{Abs, Frame, Size};

    use super::*;

    fn make_page(width: f64, height: f64, number: u64) -> Page {
        let size = Size::new(Abs::pt(width), Abs::pt(height));
        Page {
            frame: Frame::soft(size),
            fill: Smart::Auto,
            numbering: None,
            supplement: Content::empty(),
            number,
        }
    }

    #[test]
    fn empty_store_reports_zero_pages() {
        let store = DiskPageStore::new().expect("create store");
        assert_eq!(store.page_count(), 0);
    }

    #[test]
    fn from_pages_preserves_count_and_dimensions() {
        let pages = vec![
            make_page(100.0, 200.0, 1),
            make_page(300.0, 400.0, 2),
            make_page(500.0, 600.0, 3),
        ];
        let store = DiskPageStore::from_pages(&pages).expect("from_pages");
        assert_eq!(store.page_count(), 3);

        for (i, original) in pages.iter().enumerate() {
            let got = store.read_page(i).expect("read_page");
            assert_eq!(got.number, original.number);
            assert_eq!(got.frame.width(), original.frame.width());
            assert_eq!(got.frame.height(), original.frame.height());
        }
    }

    #[test]
    fn append_then_flush_then_iterate_returns_all_pages_in_order() {
        let mut store = DiskPageStore::new().expect("create store");
        let originals: Vec<_> =
            (1..=5).map(|n| make_page(100.0 + n as f64, 200.0, n)).collect();

        for page in &originals {
            store.append_page(page).expect("append_page");
        }
        store.flush_writer().expect("flush_writer");

        assert_eq!(store.page_count(), originals.len());

        let got: Vec<_> = store
            .pages_iter()
            .expect("pages_iter")
            .collect::<io::Result<Vec<_>>>()
            .expect("iterate");

        assert_eq!(got.len(), originals.len());
        for (g, o) in got.iter().zip(&originals) {
            assert_eq!(g.number, o.number);
            assert_eq!(g.frame.width(), o.frame.width());
        }
    }

    #[test]
    fn read_page_out_of_range_errors() {
        let pages = vec![make_page(100.0, 200.0, 1)];
        let store = DiskPageStore::from_pages(&pages).expect("from_pages");

        let err = store.read_page(5).expect_err("should error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn remaining_tags_inject_into_last_page_only() {
        let mut store = DiskPageStore::from_pages(&[
            make_page(100.0, 200.0, 1),
            make_page(100.0, 200.0, 2),
        ])
        .expect("from_pages");

        // No remaining tags: last page has no tag-only items.
        let last_before = store.read_page(1).expect("read last before");
        let tag_count_before = last_before
            .frame
            .items()
            .filter(|(_, item)| matches!(item, FrameItem::Tag(_)))
            .count();
        assert_eq!(tag_count_before, 0);

        // With remaining tags: injected only into the last page.
        use typst_library::introspection::{Location, Tag, TagFlags};
        let tag = Tag::End(
            Location::new(0),
            0,
            TagFlags { introspectable: false, tagged: false },
        );
        store.set_remaining_tags(vec![tag]);

        let first = store.read_page(0).expect("read first");
        let first_tag_count = first
            .frame
            .items()
            .filter(|(_, item)| matches!(item, FrameItem::Tag(_)))
            .count();
        assert_eq!(first_tag_count, 0);

        let last = store.read_page(1).expect("read last");
        let last_tag_count = last
            .frame
            .items()
            .filter(|(_, item)| matches!(item, FrameItem::Tag(_)))
            .count();
        assert_eq!(last_tag_count, 1);
    }
}
