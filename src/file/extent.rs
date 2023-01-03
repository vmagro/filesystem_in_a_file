use bytes::Bytes;

use super::File;

/// A single piece of data that makes up a file. Immutable but can be composed
/// with other Extents in order to implement mutable files on top of immutable
/// extent chunks.
#[derive(Clone, PartialEq, Eq)]
pub enum Extent {
    /// The source-of-truth for this data is the file that contains it. It
    /// originated from a write to that File, not a clone from another.
    Owned(Bytes),
    /// This extent came from part of another File.
    Cloned(Cloned),
}

impl Extent {
    pub fn len(&self) -> u64 {
        self.data().len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.data().is_empty()
    }

    pub fn data(&self) -> &[u8] {
        match self {
            Self::Owned(c) => &c,
            Self::Cloned(c) => &c.data,
        }
    }

    pub fn bytes(&self) -> Bytes {
        match self {
            Self::Owned(c) => c.clone(),
            Self::Cloned(c) => c.data.clone(),
        }
    }

    pub(super) fn split_at(&mut self, at: usize) -> Self {
        match self {
            Self::Owned(ref mut data) => {
                let right = data.split_off(at);
                Self::Owned(right)
            }
            Self::Cloned(ref mut c) => {
                let right = c.data.split_off(at);
                c.src_range = (c.src_range.0, std::cmp::min(at as u64, c.src_range.1));
                Self::Cloned(Cloned {
                    src_file: c.src_file.clone(),
                    src_range: (
                        std::cmp::max(at as u64, c.src_range.0),
                        std::cmp::min(at as u64, c.src_range.1),
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
pub struct Cloned {
    // TODO: figure out a way to reference the original file better
    pub(super) src_file: File,
    pub(super) src_range: (u64, u64),
    pub(super) data: Bytes,
}

impl std::fmt::Debug for Extent {
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

impl std::fmt::Debug for Cloned {
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

impl<T> From<T> for Extent
where
    T: Into<Bytes>,
{
    fn from(t: T) -> Self {
        Self::Owned(t.into())
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
