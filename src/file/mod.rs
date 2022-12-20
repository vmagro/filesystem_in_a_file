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

use crate::entry::Metadata;

/// A single file in the filesystem. This has a number of metadata attributes
/// alongside the file contents.
/// File contents are stored in Copy-on-Write [Extent]s that allow a [File] to
/// be a completely zero-copy reference to the underlying filesystem-in-a-file
/// but also be mutable (useful for things like BTRFS sendstreams that contain a
/// sequence of mutation operations instead of raw file contents).
#[derive(Debug, Clone, PartialEq, Eq, Default, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct File<'a> {
    pub(crate) extents: BTreeMap<u64, Extent<'a>>,
    pub(crate) metadata: Metadata<'a>,
}

impl<'a> FileBuilder<'a> {
    /// Set the contents of the [File] to a single [Extent] blob.
    pub fn contents(&mut self, contents: impl Into<Extent<'a>>) -> &mut Self {
        self.extents(BTreeMap::from([(0, contents.into())]))
    }

    pub fn build(&mut self) -> File<'a> {
        self.fallible_build().expect("infallible")
    }
}

impl<'a> File<'a> {
    pub fn builder() -> FileBuilder<'a> {
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
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.len() as usize);
        self.reader().read_to_end(&mut v).expect("infallible");
        v
    }

    /// Find the extent that contains the byte at 'pos'
    pub(self) fn extent_for_byte(&self, pos: u64) -> Option<(u64, &Extent<'a>)> {
        self.extents
            .range(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    /// See [File::extent_for_byte]
    pub(self) fn extent_for_byte_mut(&mut self, pos: u64) -> Option<(u64, &mut Extent<'a>)> {
        self.extents
            .range_mut(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    pub fn clone(&'a self, range: Range<u64>) -> Vec<Extent<'a>> {
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
                src_file: self,
                src_range: (start, end),
                data: Cow::Borrowed(
                    &ext.data()[(start - ext_start) as usize..(end - ext_start) as usize],
                ),
            });
            v.push(cloned);
        }
        v
    }
}

#[cfg(test)]
pub(self) mod tests {
    use nix::sys::stat::Mode;
    use nix::unistd::Gid;
    use nix::unistd::Uid;

    use super::*;

    pub(crate) fn test_file() -> File<'static> {
        File {
            extents: BTreeMap::from([
                (0, "Lorem ipsum".into()),
                ("Lorem ipsum".len() as u64, " dolor sit amet".into()),
            ]),
            metadata: Metadata {
                mode: Mode::from_bits_truncate(0o444),
                uid: Uid::from_raw(0),
                gid: Gid::from_raw(0),
                xattrs: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn to_bytes() {
        let f = test_file();
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet", "{f:?}");
    }

    #[test]
    fn cloning() {
        let f = test_file();
        let extents =
            f.clone("Lorem ".len() as u64.."Lorem ".len() as u64 + "ipsum dolor".len() as u64);
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
}
