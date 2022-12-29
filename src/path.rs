use std::ffi::OsStr;
use std::fmt::Debug;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use bytes::Bytes;

#[derive(Clone, Eq, Hash, PartialOrd, Ord)]
pub struct BytesPath(Bytes);

impl BytesPath {
    pub(crate) fn bytes_mut(&mut self) -> &mut Bytes {
        &mut self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Deref for BytesPath {
    type Target = Path;

    fn deref(&self) -> &Path {
        Path::new(OsStr::from_bytes(&self.0))
    }
}

impl AsRef<Path> for BytesPath {
    fn as_ref(&self) -> &Path {
        self.deref()
    }
}

impl Debug for BytesPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T> From<T> for BytesPath
where
    T: Into<Bytes>,
{
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl<T> PartialEq<T> for BytesPath
where
    T: AsRef<Path>,
{
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other.as_ref()
    }
}
