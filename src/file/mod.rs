use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Read;
use std::ops::Range;

use derive_builder::Builder;

pub mod extent;
pub mod reader;
pub mod writer;

use extent::Cloned;
use extent::Extent;

use crate::cmp::ApproxEq;
use crate::cmp::Fields;
use crate::entry::Metadata;

/// A single file in the filesystem. This has a number of metadata attributes
/// alongside the file contents.
/// File contents are stored in Copy-on-Write [Extent]s that allow a [File] to
/// be a completely zero-copy reference to the underlying filesystem-in-a-file
/// but also be mutable (useful for things like BTRFS sendstreams that contain a
/// sequence of mutation operations instead of raw file contents).
#[derive(Debug, Clone, PartialEq, Eq, Default, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct File {
    pub(crate) extents: BTreeMap<u64, Extent>,
    pub(crate) metadata: Metadata,
}

impl FileBuilder {
    /// Set the contents of the [File] to a single [Extent] blob.
    pub fn contents(&mut self, contents: impl Into<Extent>) -> &mut Self {
        self.extents(BTreeMap::from([(0, contents.into())]))
    }

    pub fn build(&mut self) -> File {
        self.fallible_build().expect("infallible")
    }
}

impl File {
    pub fn builder() -> FileBuilder {
        FileBuilder::default()
    }

    pub fn new_empty() -> Self {
        Self::builder().build()
    }

    pub fn is_empty(&self) -> bool {
        self.extents.is_empty()
    }

    pub fn len(&self) -> u64 {
        self.extents
            .last_key_value()
            .map(|(start, ext)| *start + ext.len())
            .unwrap_or(0)
    }

    /// Copy all of the extents in this file into a single contiguous array of
    /// bytes.
    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        match self.extents.len() {
            0 => Cow::Borrowed(&[]),
            1 => Cow::Borrowed(self.extents[&0].data()),
            _ => {
                let mut v = Vec::with_capacity(self.len() as usize);
                self.reader().read_to_end(&mut v).expect("infallible");
                Cow::Owned(v)
            }
        }
    }

    /// Find the extent that contains the byte at 'pos'
    pub(self) fn extent_for_byte(&self, pos: u64) -> Option<(u64, &Extent)> {
        self.extents
            .range(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    /// See [File::extent_for_byte]
    pub(self) fn extent_for_byte_mut(&mut self, pos: u64) -> Option<(u64, &mut Extent)> {
        self.extents
            .range_mut(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    pub fn clone_range(&self, range: Range<u64>) -> Vec<Extent> {
        let mut v = Vec::new();
        let (start, _) = self.extent_for_byte(range.start).expect("invalid range");
        for (ext_start, ext) in self.extents.range(start..).take_while(|(start, ext)| {
            [
                std::cmp::max(**start, range.start),
                std::cmp::min(**start + (ext.len()), range.end),
            ]
            .iter()
            .any(|point| range.contains(point))
        }) {
            let start = std::cmp::max(range.start, *ext_start);
            let end = std::cmp::min(range.end, ext_start + ext.len());
            let cloned = Extent::Cloned(Cloned {
                src_file: self.clone(),
                src_range: (start, end),
                data: ext
                    .bytes()
                    .slice((start - ext_start) as usize..(end - ext_start) as usize),
            });
            v.push(cloned);
        }
        v
    }

    /// Force the file length to be this value. Extents are shrunk or deleted if
    /// the new size is smaller. If the new size is larger, an extent of
    /// all-zeroes is created at the end of the file
    pub fn truncate(&mut self, len: u64) {
        if len < self.len() {
            self.extents.retain(|k, _| *k < len);
            let last_start = self.extents.last_key_value().map(|(k, _)| *k);
            if let Some(start) = last_start {
                let pos = len - start;
                self.extents
                    .get_mut(&start)
                    .expect("definitely exists")
                    .split_at(pos as usize);
            }
        } else {
            self.extents
                .insert(self.len(), Extent::Hole(len - self.len()));
        }
    }
}

impl ApproxEq for File {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> Fields {
        let Self { metadata, extents } = self;
        let mut f = metadata.cmp(&other.metadata);
        if *extents != other.extents {
            f.remove(Fields::EXTENTS);
        }
        if self.to_bytes() != other.to_bytes() {
            f.remove(Fields::DATA);
        }
        f
    }
}

#[cfg(test)]
pub(self) mod tests {
    use super::*;

    pub(crate) fn test_file() -> File {
        File {
            extents: BTreeMap::from([
                (0, "Lorem ipsum".into()),
                ("Lorem ipsum".len() as u64, " dolor sit amet".into()),
            ]),
            metadata: Default::default(),
        }
    }

    #[test]
    fn to_bytes() {
        let f = test_file();
        assert_eq!(
            f.to_bytes().as_ref(),
            b"Lorem ipsum dolor sit amet",
            "{f:?}"
        );
    }

    #[test]
    fn cloning() {
        let f = test_file();
        let extents = f
            .clone_range("Lorem ".len() as u64.."Lorem ".len() as u64 + "ipsum dolor".len() as u64);
        let mut f2 = File::new_empty();
        let mut w = f2.writer();
        assert_eq!(extents.len(), 2, "{extents:?}");
        for ex in extents {
            w.write(ex)
        }
        assert_eq!(
            std::str::from_utf8(&f2.to_bytes()).expect("valid"),
            "ipsum dolor",
            "{f2:?}"
        );
    }

    #[test]
    fn truncate() {
        let mut f = test_file();
        f.truncate(5);
        assert_eq!(f.to_bytes().as_ref(), b"Lorem");
        assert_eq!(f.extents.len(), 1);

        let mut f = test_file();
        f.truncate(("Lorem ipsum dolor sit amet".len() + 128) as u64);
        assert_eq!(f.len(), ("Lorem ipsum dolor sit amet".len() + 128) as u64);
        assert_eq!(f.extents.len(), 3);
    }
}
