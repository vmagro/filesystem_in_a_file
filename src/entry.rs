use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;

use bytes::Bytes;
use derive_builder::Builder;
use getset::CopyGetters;
use getset::Getters;
use nix::sys::stat::FileStat;
use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::BytesPath;
use crate::File;

/// A single directory entry in the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
#[remain::sorted]
pub enum Entry {
    Directory(Directory),
    /// A regular file
    File(File),
    Special(Special),
    Symlink(Symlink),
}

impl Entry {
    pub fn metadata(&self) -> &Metadata {
        #[remain::sorted]
        match self {
            Self::Directory(d) => &d.metadata,
            Self::File(f) => &f.metadata,
            Self::Special(s) => &s.metadata,
            Self::Symlink(s) => &s.metadata,
        }
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata {
        #[remain::sorted]
        match self {
            Self::Directory(d) => &mut d.metadata,
            Self::File(f) => &mut f.metadata,
            Self::Special(s) => &mut s.metadata,
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
    #[get_copy = "pub"]
    pub(crate) created: SystemTime,
    #[get_copy = "pub"]
    pub(crate) accessed: SystemTime,
    #[get_copy = "pub"]
    pub(crate) modified: SystemTime,
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

    pub fn set_times(&mut self, created: SystemTime, accessed: SystemTime, modified: SystemTime) {
        self.created = created;
        self.accessed = accessed;
        self.modified = modified;
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            mode: Mode::from_bits_truncate(0o444),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
            created: SystemTime::UNIX_EPOCH,
            accessed: SystemTime::UNIX_EPOCH,
            modified: SystemTime::UNIX_EPOCH,
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
            created: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.st_ctime.try_into().expect("must be positive"))
                + Duration::from_nanos(fs.st_ctime_nsec.try_into().expect("must be positive")),
            accessed: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.st_atime.try_into().expect("must be positive"))
                + Duration::from_nanos(fs.st_atime_nsec.try_into().expect("must be positive")),
            modified: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.st_mtime.try_into().expect("must be positive"))
                + Duration::from_nanos(fs.st_mtime_nsec.try_into().expect("must be positive")),
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
            created: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.ctime().try_into().expect("must be positive"))
                + Duration::from_nanos(fs.ctime_nsec().try_into().expect("must be positive")),
            accessed: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.atime().try_into().expect("must be positive"))
                + Duration::from_nanos(fs.atime_nsec().try_into().expect("must be positive")),
            modified: SystemTime::UNIX_EPOCH
                + Duration::from_secs(fs.mtime().try_into().expect("must be positive"))
                + Duration::from_nanos(fs.mtime_nsec().try_into().expect("must be positive")),
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

/// A special file (device node, socket, fifo, etc)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Special {
    /// Special file type
    file_type: SFlag,
    metadata: Metadata,
}

impl Special {
    pub fn new(file_type: SFlag, metadata: Metadata) -> Self {
        Self {
            file_type,
            metadata,
        }
    }

    pub fn file_type(&self) -> SFlag {
        self.file_type
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
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
