use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::Read;
use std::ops::Range;

use derive_builder::Builder;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;

pub mod reader;
pub mod writer;

#[derive(Debug, Clone, PartialEq, Eq, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct File<'a> {
    extents: BTreeMap<usize, Extent<'a>>,
    mode: Mode,
    uid: Uid,
    gid: Gid,
    xattrs: BTreeMap<Cow<'a, OsStr>, Cow<'a, [u8]>>,
}

impl<'a> FileBuilder<'a> {
    pub fn contents(&mut self, contents: impl Into<Extent<'a>>) -> &mut Self {
        self.extents(BTreeMap::from([(0, contents.into())]))
    }

    pub fn build(&mut self) -> File<'a> {
        self.fallible_build().expect("infallible")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Extent<'a> {
    /// The source-of-truth for this data is the file that contains it. It
    /// originated from a write to that File, not a clone from another.
    Owned(Cow<'a, [u8]>),
    /// This extent came from part of another File.
    Cloned(Cloned<'a>),
}

impl<'a> Extent<'a> {
    pub fn len(&self) -> usize {
        self.data().len()
    }

    pub fn is_empty(&self) -> bool {
        self.data().is_empty()
    }

    pub fn data(&self) -> &[u8] {
        match self {
            Self::Owned(c) => c,
            Self::Cloned(c) => &c.data,
        }
    }

    fn split_at(&mut self, pos: usize) -> Extent<'a> {
        match self {
            Self::Owned(cow) => Self::Owned(split_cow_in_place(cow, pos)),
            Self::Cloned(c) => {
                let right = split_cow_in_place(&mut c.data, pos);
                Self::Cloned(Cloned {
                    src_file: c.src_file,
                    src_range: (
                        std::cmp::max(pos, c.src_range.0),
                        std::cmp::min(pos, c.src_range.1),
                    ),
                    data: right,
                })
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Cloned<'a> {
    src_file: &'a File<'a>,
    src_range: (usize, usize),
    data: Cow<'a, [u8]>,
}

fn split_cow_in_place<'a>(cow: &mut Cow<'a, [u8]>, pos: usize) -> Cow<'a, [u8]> {
    match *cow {
        Cow::Owned(ref mut d) => {
            let right = d[pos..].to_vec();
            d.truncate(pos);
            Cow::Owned(right)
        }
        Cow::Borrowed(d) => {
            let (left, right) = d.split_at(pos);
            *cow = Cow::Borrowed(left);
            Cow::Borrowed(right)
        }
    }
}

impl<'a> std::fmt::Debug for Extent<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Owned(o) => {
                let mut d = f.debug_tuple("Owned");
                match std::str::from_utf8(o) {
                    Ok(s) => {
                        d.field(&s);
                    }
                    Err(_) => {
                        d.field(&self.data());
                    }
                }
                d.finish()
            }
            Self::Cloned(c) => f.debug_tuple("Cloned").field(&c).finish(),
        }
    }
}

impl<'a> std::fmt::Debug for Cloned<'a> {
    #[deny(unused_variables)]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Self {
            src_file,
            src_range,
            data,
        } = self;
        let mut d = f.debug_struct("Cloned");
        d.field("src_file", &src_file);
        d.field("src_range", &src_range);
        match std::str::from_utf8(data) {
            Ok(s) => {
                d.field("data", &s);
            }
            Err(_) => {
                d.field("data", data);
            }
        };
        d.finish()
    }
}

impl<'a> From<&'a [u8]> for Extent<'a> {
    fn from(data: &'a [u8]) -> Self {
        Self::Owned(Cow::Borrowed(data))
    }
}

impl<'a> From<&'a str> for Extent<'a> {
    fn from(s: &'a str) -> Self {
        Self::Owned(Cow::Borrowed(s.as_bytes()))
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for Extent<'a> {
    fn from(data: &'a [u8; N]) -> Self {
        Extent::from(&data[..])
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

    pub fn len(&self) -> usize {
        self.extents
            .last_key_value()
            .map(|(start, ext)| *start + ext.len())
            .unwrap_or(0)
    }

    /// Copy all of the extents in this file into a single contiguous array of
    /// bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.len());
        self.reader().read_to_end(&mut v).expect("infallible");
        v
    }

    /// Find the extent that contains the byte at 'pos'
    pub(self) fn extent_for_byte(&self, pos: usize) -> Option<(usize, &Extent<'a>)> {
        self.extents
            .range(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    /// See [File::extent_for_byte]
    pub(self) fn extent_for_byte_mut(&mut self, pos: usize) -> Option<(usize, &mut Extent<'a>)> {
        self.extents
            .range_mut(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
            .filter(|(start, e)| pos <= start + e.len())
    }

    pub fn clone(&'a self, range: Range<usize>) -> Vec<Extent<'a>> {
        let mut v = Vec::new();
        for (ext_start, ext) in self.extents.range(range.clone()) {
            let start = std::cmp::max(range.start, *ext_start);
            let end = std::cmp::min(range.end, ext_start + ext.len());
            let cloned = Extent::Cloned(Cloned {
                src_file: self,
                src_range: (start, end),
                data: Cow::Borrowed(&ext.data()[start - ext_start..end - ext_start]),
            });
            v.push(cloned);
        }
        v
    }
}

impl<'a> Default for File<'a> {
    fn default() -> Self {
        Self {
            extents: BTreeMap::new(),
            mode: Mode::from_bits_truncate(0o444),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
pub(self) mod tests {
    use super::*;

    pub(crate) fn test_file() -> File<'static> {
        File {
            extents: BTreeMap::from([
                (0, "Lorem ipsum".into()),
                ("Lorem ipsum".len(), " dolor sit amet".into()),
            ]),
            mode: Mode::from_bits_truncate(0o444),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
        }
    }

    #[test]
    fn to_bytes() {
        let f = test_file();
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet", "{f:?}");
    }

    #[test]
    fn extent_split() {
        let mut ext: Extent = "Lorem ipsum".into();
        assert_eq!(ext, "Lorem ipsum".into());
        let right = ext.split_at("Lorem".len());
        let left = ext;
        assert_eq!(left, "Lorem".into());
        assert_eq!(right, " ipsum".into());
    }

    #[test]
    fn cloning() {
        let f = test_file();
        let extents = f.clone(0..5);
        let mut f2 = File::new_empty();
        let mut w = f2.writer();
        for ex in extents {
            w.write(ex)
        }
        assert_eq!(
            std::str::from_utf8(&f2.to_bytes()).expect("valid"),
            "Lorem",
            "{f2:?}"
        );
    }
}
