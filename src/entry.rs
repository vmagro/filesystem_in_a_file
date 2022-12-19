use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;

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

    pub fn xattrs<'a>(&'a self) -> &BTreeMap<Cow<'a, OsStr>, Cow<'a, [u8]>> {
        match self {
            Self::File(f) => &f.xattrs,
            Self::Directory(d) => &d.xattrs,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory<'a> {
    mode: Mode,
    uid: Uid,
    gid: Gid,
    xattrs: BTreeMap<Cow<'a, OsStr>, Cow<'a, [u8]>>,
}
