use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use derive_builder::Builder;
use getset::CopyGetters;
use getset::Getters;
use nix::sys::stat::FileStat;
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
    Symlink(Symlink<'f>),
}

impl<'f> Entry<'f> {
    pub fn metadata(&self) -> &Metadata {
        match self {
            Self::File(f) => &f.metadata,
            Self::Directory(d) => &d.metadata,
            Self::Symlink(s) => &s.metadata,
        }
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata<'f> {
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

    pub fn set_xattr(
        &mut self,
        name: &'f OsStr,
        val: &'f [u8],
    ) -> Option<&'f [u8]> {
        self.metadata_mut().xattrs.insert(name, val)
    }

    pub fn remove_xattr(&mut self, name: &'f OsStr) -> Option<&'f [u8]> {
        self.metadata_mut().xattrs.remove(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Metadata<'f> {
    #[get_copy = "pub"]
    pub(crate) mode: Mode,
    #[get_copy = "pub"]
    pub(crate) uid: Uid,
    #[get_copy = "pub"]
    pub(crate) gid: Gid,
    #[get = "pub"]
    pub(crate) xattrs: BTreeMap<&'f OsStr, &'f [u8]>,
}

impl<'f> Metadata<'f> {
    pub fn builder() -> MetadataBuilder<'f> {
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

    /// Some of our supported archive formats don't support xattrs, so make it
    /// easy to remove them from the demo test data
    #[cfg(all(test, any(feature = "cpio")))]
    pub(crate) fn clear_xattrs(&mut self) {
        self.xattrs.clear()
    }
}

impl<'f> Default for Metadata<'f> {
    fn default() -> Self {
        Self {
            mode: Mode::from_bits_truncate(0o444),
            uid: Uid::from_raw(0),
            gid: Gid::from_raw(0),
            xattrs: BTreeMap::new(),
        }
    }
}

impl<'f> From<FileStat> for Metadata<'f> {
    fn from(fs: FileStat) -> Self {
        Self {
            mode: Mode::from_bits_truncate(fs.st_mode),
            uid: Uid::from_raw(fs.st_uid),
            gid: Gid::from_raw(fs.st_gid),
            xattrs: BTreeMap::new(),
        }
    }
}

impl<'f> From<std::fs::Metadata> for Metadata<'f> {
    fn from(fs: std::fs::Metadata) -> Self {
        Self {
            mode: Mode::from_bits_truncate(fs.mode()),
            uid: Uid::from_raw(fs.uid()),
            gid: Gid::from_raw(fs.gid()),
            xattrs: BTreeMap::new(),
        }
    }
}

impl<'f> MetadataBuilder<'f> {
    /// Add a single xattr
    pub fn xattr(
        &mut self,
        name: &'f OsStr,
        value: &'f [u8]
    ) -> &mut Self {
        if self.xattrs.is_none() {
            self.xattrs = Some(BTreeMap::new());
        }
        self.xattrs
            .as_mut()
            .expect("this is Some")
            .insert(name, value);
        self
    }

    pub fn build(&mut self) -> Metadata<'f> {
        self.fallible_build().expect("infallible")
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

impl<'f> From<Symlink<'f>> for Entry<'f> {
    fn from(s: Symlink<'f>) -> Self {
        Self::Symlink(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Directory<'f> {
    metadata: Metadata<'f>,
}

impl<'f> Directory<'f> {
    pub fn builder() -> DirectoryBuilder<'f> {
        Default::default()
    }
}

impl<'f> DirectoryBuilder<'f> {
    pub fn build(&mut self) -> Directory<'f> {
        self.fallible_build().expect("infallible")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters)]
pub struct Symlink<'f> {
    #[get = "pub"]
    /// Target path
    target: &'f Path,
    metadata: Metadata<'f>,
}

impl<'f> Symlink<'f> {
    pub fn new(target: &'f Path, metadata: Option<Metadata<'f>>) -> Self {
        Self {
            target: target.into(),
            metadata: metadata.unwrap_or_default(),
        }
    }
}
