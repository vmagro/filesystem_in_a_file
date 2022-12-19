use std::borrow::Cow;

use super::File;

/// A single piece of data that makes up a file. Immutable but can be composed
/// with other Extents in order to implement mutable files on top of immutable
/// extent chunks.
#[derive(Clone, PartialEq, Eq)]
pub enum Extent<'a> {
    /// The source-of-truth for this data is the file that contains it. It
    /// originated from a write to that File, not a clone from another.
    Owned(Cow<'a, [u8]>),
    /// This extent came from part of another File.
    Cloned(Cloned<'a>),
}

impl<'a> Extent<'a> {
    pub fn len(&self) -> u64 {
        self.data().len() as u64
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

    pub(super) fn split_at(&mut self, pos: usize) -> Extent<'a> {
        match self {
            Self::Owned(cow) => Self::Owned(split_cow_in_place(cow, pos)),
            Self::Cloned(c) => {
                let right = split_cow_in_place(&mut c.data, pos);
                Self::Cloned(Cloned {
                    src_file: c.src_file,
                    src_range: (
                        std::cmp::max(pos as u64, c.src_range.0),
                        std::cmp::min(pos as u64, c.src_range.1),
                    ),
                    data: right,
                })
            }
        }
    }
}

/// A Cloned [Extent] comes from another file. This extent references the
/// original [File] and the location in that file for debuggability of BTRFS
/// sendstreams.
#[derive(Clone, PartialEq, Eq)]
pub struct Cloned<'a> {
    pub(super) src_file: &'a File<'a>,
    pub(super) src_range: (u64, u64),
    pub(super) data: Cow<'a, [u8]>,
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

impl<'a> From<Vec<u8>> for Extent<'a> {
    fn from(data: Vec<u8>) -> Self {
        Self::Owned(Cow::Owned(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_split() {
        let mut ext: Extent = "Lorem ipsum".into();
        assert_eq!(ext, "Lorem ipsum".into());
        let right = ext.split_at("Lorem".len());
        let left = ext;
        assert_eq!(left, "Lorem".into());
        assert_eq!(right, " ipsum".into());
    }
}
