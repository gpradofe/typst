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
            let bytes = bincode::serialize(&spage)
                .map_err(io::Error::other)?;
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
        let bytes = bincode::serialize(&spage)
            .map_err(io::Error::other)?;

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

    /// Sets remaining tags to be injected into the last page when read.
    pub fn set_remaining_tags(&mut self, tags: Vec<Tag>) {
        self.remaining_tags = tags;
    }

    /// Reads a single page back from disk and reconstructs it.
    pub fn read_page(&self, index: usize) -> io::Result<Page> {
        if index >= self.page_count {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "page index out of range"));
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
                self.remaining_tags.iter().cloned().map(|tag| (pos, FrameItem::Tag(tag))),
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
            Some(Some(paint)) => Smart::Custom(Some(self.converter.reconstruct_paint(paint))),
        };

        let numbering = spage.numbering_ref.map(|id| {
            self.numberings[id as usize].clone()
        });

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
                    self.store.remaining_tags.iter().cloned().map(|tag| (pos, FrameItem::Tag(tag))),
                );
            }

            Ok(page)
        })();

        self.index += 1;
        Some(result)
    }
}
