use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;

use derive_builder::Builder;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::File;

/// A single directory entry in the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry<'f> {
    /// A regular file
    File(File<'f>),
    Directory(Directory<'f>),
}

impl<'f> Entry<'f> {
    pub fn permissions(&self) -> std::fs::Permissions {
        match self {
            Self::File(f) => std::fs::Permissions::from_mode(f.mode.bits()),
            Self::Directory(d) => std::fs::Permissions::from_mode(d.mode.bits()),
        }
    }

    pub fn uid(&self) -> Uid {
        match self {
            Self::File(f) => f.uid,
            Self::Directory(d) => d.uid,
        }
    }

    pub fn gid(&self) -> Gid {
        match self {
            Self::File(f) => f.gid,
            Self::Directory(d) => d.gid,
        }
    }

    pub fn xattrs(&self) -> &BTreeMap<Cow<'_, OsStr>, Cow<'_, [u8]>> {
        match self {
            Self::File(f) => &f.xattrs,
            Self::Directory(d) => &d.xattrs,
        }
    }

    /// Some of our supported archive formats don't support xattrs, so make it
    /// easy to remove them from the demo test data
    #[cfg(test)]
    pub(crate) fn clear_xattrs(&mut self) {
        match self {
            Self::File(f) => f.xattrs.clear(),
            Self::Directory(d) => d.xattrs.clear(),
        }
    }
}

impl<'f> From<File<'f>> for Entry<'f> {
    fn from(f: File<'f>) -> Self {
        Self::File(f)
    }
}

impl<'f> From<Directory<'f>> for Entry<'f> {
    fn from(d: Directory<'f>) -> Self {
        Self::Directory(d)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Directory<'a> {
    mode: Mode,
    uid: Uid,
    gid: Gid,
    xattrs: BTreeMap<Cow<'a, OsStr>, Cow<'a, [u8]>>,
}

impl<'a> Directory<'a> {
    pub fn builder() -> DirectoryBuilder<'a> {
        DirectoryBuilder::default()
    }
}

impl<'a> DirectoryBuilder<'a> {
    /// Add a single xattr to the [Directory]
    pub fn xattr(
        &mut self,
        name: impl Into<Cow<'a, OsStr>>,
        value: impl Into<Cow<'a, [u8]>>,
    ) -> &mut Self {
        if self.xattrs.is_none() {
            self.xattrs = Some(BTreeMap::new());
        }
        self.xattrs
            .as_mut()
            .expect("this is Some")
            .insert(name.into(), value.into());
        self
    }

    pub fn build(&mut self) -> Directory<'a> {
        self.fallible_build().expect("infallible")
    }
}

impl<'a> Default for Directory<'a> {
    fn default() -> Self {
        Self {
            mode: Mode::from_bits_truncate(0o555),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
        }
    }
}
