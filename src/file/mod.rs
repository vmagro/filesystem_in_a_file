use std::borrow::Cow;
use std::collections::BTreeMap;

pub mod reader;
pub mod writer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File<'a> {
    extents: BTreeMap<usize, Extent<'a>>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Extent<'a> {
    /// The source-of-truth for this data is the file that contains it. It
    /// originated from a write to that File, not a clone from another.
    Owned(Cow<'a, [u8]>),
}

impl<'a> Extent<'a> {
    pub fn len(&self) -> usize {
        self.data().len()
    }

    pub fn data(&self) -> &[u8] {
        match self {
            Self::Owned(c) => c,
        }
    }
}

impl<'a> std::fmt::Debug for Extent<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut d = f.debug_tuple(match self {
            Self::Owned(_) => "Owned",
        });

        match std::str::from_utf8(self.data()) {
            Ok(s) => {
                d.field(&s);
            }
            Err(_) => {
                d.field(&self.data());
            }
        }

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

impl<'a> File<'a> {
    /// Create a new, empty File
    pub fn new() -> Self {
        Self {
            extents: BTreeMap::new(),
        }
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
        let mut v = vec![0; self.len()];
        for (start, extent) in &self.extents {
            let end = *start + extent.len();
            v[*start..end].copy_from_slice(extent.data());
        }
        v
    }

    /// Find the extent that contains the byte at 'pos'
    pub(self) fn extent_for_byte(&self, pos: usize) -> Option<(usize, &Extent<'a>)> {
        self.extents
            .range(..pos + 1)
            .next_back()
            .map(|(start, e)| (*start, e))
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
        }
    }

    #[test]
    fn to_bytes() {
        let f = test_file();
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet", "{:?}", f);
    }
}
