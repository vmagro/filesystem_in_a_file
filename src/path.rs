use std::borrow::Borrow;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;

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

    pub fn as_path(&self) -> &Path {
        self
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

impl<T> PartialEq<T> for BytesPath
where
    T: AsRef<Path>,
{
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl From<Bytes> for BytesPath {
    fn from(value: Bytes) -> Self {
        Self(value)
    }
}

impl From<&'static str> for BytesPath {
    fn from(value: &'static str) -> Self {
        Self(value.into())
    }
}

impl From<&'static [u8]> for BytesPath {
    fn from(value: &'static [u8]) -> Self {
        Self(value.into())
    }
}

impl From<&Path> for BytesPath {
    fn from(value: &Path) -> Self {
        Self(Bytes::copy_from_slice(value.as_os_str().as_bytes()))
    }
}

impl From<PathBuf> for BytesPath {
    fn from(value: PathBuf) -> Self {
        Self(Bytes::copy_from_slice(value.as_os_str().as_bytes()))
    }
}

impl Borrow<Path> for BytesPath {
    fn borrow(&self) -> &Path {
        self
    }
}

impl Borrow<Path> for &BytesPath {
    fn borrow(&self) -> &Path {
        self
    }
}

impl Borrow<str> for BytesPath {
    fn borrow(&self) -> &str {
        std::str::from_utf8(&self.0).expect("all paths we will deal with are utf8")
    }
}
