use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;

use bytes::Bytes;
use derive_builder::Builder;
use derive_more::From;
use derive_more::IsVariant;
use getset::CopyGetters;
use getset::Getters;
use nix::sys::stat::FileStat;
use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::cmp::ApproxEq;
use crate::cmp::Fields;
use crate::BytesPath;
use crate::File;

/// A single directory entry in the filesystem.
#[derive(Debug, Clone, PartialEq, Eq, From, IsVariant)]
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

impl ApproxEq for Entry {
    fn cmp(&self, other: &Self) -> Fields {
        let f = self.metadata().cmp(other.metadata());
        match (self, other) {
            (Self::Directory(s), Self::Directory(o)) => f.intersection(s.cmp(o)),
            (Self::Directory(_), _) => f - Fields::TYPE,
            (Self::File(s), Self::File(o)) => f.intersection(s.cmp(o)),
            (Self::File(_), _) => f - Fields::TYPE,
            (Self::Special(_), Self::Special(_)) => self.metadata().cmp(other.metadata()),
            (Self::Special(_), _) => f - Fields::TYPE,
            (Self::Symlink(s), Self::Symlink(o)) => f.intersection(s.cmp(o)),
            (Self::Symlink(_), _) => f - Fields::TYPE,
        }
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

impl ApproxEq for Metadata {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> Fields {
        let Self {
            mode,
            uid,
            gid,
            xattrs,
            created,
            accessed,
            modified,
        } = self;
        let mut f = Fields::all();
        if *mode != other.mode {
            f.remove(Fields::MODE);
        }
        if *uid != other.uid || *gid != other.gid {
            f.remove(Fields::OWNER);
        }
        if *xattrs != other.xattrs {
            f.remove(Fields::XATTR);
        }
        if *created != other.created || *accessed != other.accessed || *modified != other.modified {
            f.remove(Fields::TIME);
        }
        f
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

impl ApproxEq for Directory {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> Fields {
        let Self { metadata } = self;
        metadata.cmp(&other.metadata)
    }
}

/// A special file (device node, socket, fifo, etc)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Special {
    /// Special file type
    file_type: SFlag,
    rdev: u64,
    metadata: Metadata,
}

impl Special {
    pub fn new(file_type: SFlag, rdev: u64, metadata: Metadata) -> Self {
        Self {
            file_type,
            rdev,
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

impl ApproxEq for Special {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> Fields {
        let Self {
            file_type,
            metadata,
            rdev,
        } = self;
        let mut f = metadata.cmp(&other.metadata);
        if *file_type != other.file_type {
            f.remove(Fields::TYPE);
        }
        if *rdev != other.rdev {
            f.remove(Fields::RDEV);
        }
        f
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
        let metadata = metadata.unwrap_or_else(|| {
            Metadata::builder()
                .mode(Mode::from_bits_truncate(0o777))
                .build()
        });
        Self {
            target: target.into(),
            metadata,
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

impl ApproxEq for Symlink {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> Fields {
        let Self { target, metadata } = self;
        let mut f = metadata.cmp(&other.metadata);
        if *target != other.target {
            f.remove(Fields::DATA);
        }
        f
    }
}
