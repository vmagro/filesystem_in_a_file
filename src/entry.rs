use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;

use derive_builder::Builder;
use getset::CopyGetters;
use getset::Getters;
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
    pub fn metadata(&self) -> &Metadata {
        match self {
            Self::File(f) => &f.metadata,
            Self::Directory(d) => &d.metadata,
        }
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata<'f> {
        match self {
            Self::File(f) => &mut f.metadata,
            Self::Directory(d) => &mut d.metadata,
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
        name: impl Into<Cow<'f, OsStr>>,
        val: impl Into<Cow<'f, [u8]>>,
    ) -> Option<Cow<'f, [u8]>> {
        self.metadata_mut().xattrs.insert(name.into(), val.into())
    }

    pub fn remove_xattr(&mut self, name: &'f OsStr) -> Option<Cow<'f, [u8]>> {
        self.metadata_mut().xattrs.remove(&Cow::Borrowed(name))
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
    pub(crate) xattrs: BTreeMap<Cow<'f, OsStr>, Cow<'f, [u8]>>,
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
    #[cfg(test)]
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

impl<'f> MetadataBuilder<'f> {
    /// Add a single xattr
    pub fn xattr(
        &mut self,
        name: impl Into<Cow<'f, OsStr>>,
        value: impl Into<Cow<'f, [u8]>>,
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Builder)]
#[builder(default, setter(into), build_fn(private, name = "fallible_build"))]
pub struct Directory<'f> {
    metadata: Metadata<'f>,
}

impl<'f> Directory<'f> {
    pub fn builder() -> DirectoryBuilder<'f> {
        DirectoryBuilder::default()
    }
}

impl<'f> DirectoryBuilder<'f> {
    pub fn build(&mut self) -> Directory<'f> {
        self.fallible_build().expect("infallible")
    }
}
