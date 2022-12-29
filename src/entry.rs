use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use bytes::Bytes;
use derive_builder::Builder;
use getset::CopyGetters;
use getset::Getters;
use nix::sys::stat::FileStat;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::BytesPath;
use crate::File;

/// A single directory entry in the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A regular file
    File(File),
    Directory(Directory),
    Symlink(Symlink),
}

impl Entry {
    pub fn metadata(&self) -> &Metadata {
        match self {
            Self::File(f) => &f.metadata,
            Self::Directory(d) => &d.metadata,
            Self::Symlink(s) => &s.metadata,
        }
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata {
        match self {
            Self::File(f) => &mut f.metadata,
            Self::Directory(d) => &mut d.metadata,
            Self::Symlink(s) => &mut s.metadata,
        }
    }

    pub fn chown(&mut self, uid: Uid, gid: Gid) {
        self.metadata_mut().chown(uid, gid);
    }

    pub fn chmod(&mut self, mode: Mode) {
        self.metadata_mut().chmod(mode)
    }

    pub fn set_xattr(&mut self, name: impl Into<Bytes>, val: impl Into<Bytes>) -> Option<Bytes> {
        self.metadata_mut().xattrs.insert(name.into(), val.into())
    }

    pub fn remove_xattr(&mut self, name: &Bytes) -> Option<Bytes> {
        self.metadata_mut().xattrs.remove(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Metadata {
    #[get_copy = "pub"]
    pub(crate) mode: Mode,
    #[get_copy = "pub"]
    pub(crate) uid: Uid,
    #[get_copy = "pub"]
    pub(crate) gid: Gid,
    #[get = "pub"]
    pub(crate) xattrs: BTreeMap<Bytes, Bytes>,
}

impl Metadata {
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::default()
    }

    pub fn permissions(&self) -> std::fs::Permissions {
        std::fs::Permissions::from_mode(self.mode.bits())
    }

    pub fn chown(&mut self, uid: Uid, gid: Gid) {
        self.uid = uid;
        self.gid = gid;
    }

    pub fn chmod(&mut self, mode: Mode) {
        self.mode = mode;
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            mode: Mode::from_bits_truncate(0o444),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
        }
    }
}

impl From<FileStat> for Metadata {
    fn from(fs: FileStat) -> Self {
        Self {
            mode: Mode::from_bits_truncate(fs.st_mode),
            uid: Uid::from_raw(fs.st_uid),
            gid: Gid::from_raw(fs.st_gid),
            xattrs: BTreeMap::new(),
        }
    }
}

impl From<std::fs::Metadata> for Metadata {
    fn from(fs: std::fs::Metadata) -> Self {
        Self {
            mode: Mode::from_bits_truncate(fs.mode()),
            uid: Uid::from_raw(fs.uid()),
            gid: Gid::from_raw(fs.gid()),
            xattrs: BTreeMap::new(),
        }
    }
}

impl MetadataBuilder {
    /// Add a single xattr
    pub fn xattr(&mut self, name: impl Into<Bytes>, value: impl Into<Bytes>) -> &mut Self {
        if self.xattrs.is_none() {
            self.xattrs = Some(BTreeMap::new());
        }
        self.xattrs
            .as_mut()
            .expect("this is Some")
            .insert(name.into(), value.into());
        self
    }

    pub fn build(&mut self) -> Metadata {
        self.fallible_build().expect("infallible")
    }
}

impl From<File> for Entry {
    fn from(f: File) -> Self {
        Self::File(f)
    }
}

impl From<Directory> for Entry {
    fn from(d: Directory) -> Self {
        Self::Directory(d)
    }
}

impl From<Symlink> for Entry {
    fn from(s: Symlink) -> Self {
        Self::Symlink(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Directory {
    metadata: Metadata,
}

impl Directory {
    pub fn builder() -> DirectoryBuilder {
        Default::default()
    }
}

impl DirectoryBuilder {
    pub fn build(&mut self) -> Directory {
        self.fallible_build().expect("infallible")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symlink {
    /// Target path
    target: BytesPath,
    metadata: Metadata,
}

impl Symlink {
    pub fn new(target: impl Into<BytesPath>, metadata: Option<Metadata>) -> Self {
        Self {
            target: target.into(),
            metadata: metadata.unwrap_or_default(),
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}
