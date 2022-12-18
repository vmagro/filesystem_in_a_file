use std::borrow::Cow;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File<'a> {
    extents: BTreeMap<(usize, usize), Extent<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extent<'a> {
    /// The source-of-truth for this data is the file that contains it. It
    /// originated from a write to that File, not a clone from another.
    Owned(Cow<'a, [u8]>),
}

impl<'a> Extent<'a> {
    fn data(&self) -> &[u8] {
        match self {
            Self::Owned(c) => c,
        }
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
    pub fn is_empty(&self) -> bool {
        self.extents.is_empty()
    }

    pub fn len(&self) -> usize {
        self.extents
            .last_key_value()
            .map(|((_, end), _)| *end)
            .unwrap_or(0)
    }

    /// Copy all of the extents in this file into a single contiguous array of
    /// bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = vec![0; self.len()];
        for ((start, end), extent) in &self.extents {
            v[*start..*end].copy_from_slice(extent.data());
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file() -> File<'static> {
        File {
            extents: BTreeMap::from([
                ((0, "Lorem ipsum".len()), "Lorem ipsum".into()),
                (
                    ("Lorem ipsum".len(), "Lorem ipsum dolor sit amet".len()),
                    " dolor sit amet".into(),
                ),
            ]),
        }
    }

    #[test]
    fn to_bytes() {
        let f = test_file();
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet", "{:?}", f);
    }
}
