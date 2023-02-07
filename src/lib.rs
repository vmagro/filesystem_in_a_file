//! In-memory representation of a filesystem designed to enable full-filesystem
//! comparisons during integration tests.
//! Great care is taken to ensure that all the structs in this crate are
//! zero-copy, allowing a user to read (or better yet, mmap) a
//! filesystem-in-a-file (such as a tarball, BTRFS sendstream, cpio archive,
//! etc) and get a complete picture of the entire FS (or at least the parts that
//! can be represented in the archive format).

#![feature(io_error_more)]
#![feature(io_error_other)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(unix_chown)]

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::path::Path;
use std::time::SystemTime;

use nix::sys::stat::Mode;
use slotmap::SecondaryMap;
use slotmap::SlotMap;

#[cfg(feature = "archive")]
pub mod archive;
#[cfg(feature = "btrfs")]
pub mod btrfs;
mod bytes_ext;
pub mod cmp;
#[cfg(feature = "diff")]
pub mod diff;
mod entry;
pub mod file;
mod iter;
mod path;

pub(crate) use bytes_ext::BytesExt;
pub use entry::Entry;
use file::File;
pub use path::BytesPath;

slotmap::new_key_type! { pub struct InodeKey; }

/// Full view of a filesystem.
#[derive(Clone)]
pub struct Filesystem {
    inodes: SlotMap<InodeKey, Entry>,
    refcounts: SecondaryMap<InodeKey, usize>,
    paths: BTreeMap<BytesPath, InodeKey>,
}

impl Filesystem {
    pub fn new() -> Self {
        Self {
            inodes: SlotMap::with_key(),
            refcounts: SecondaryMap::new(),
            paths: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: impl Into<BytesPath>, entry: impl Into<Entry>) -> InodeKey {
        let key = self.inodes.insert(entry.into());
        self.paths.insert(path.into(), key);
        self.refcounts.insert(key, 1);
        key
    }

    pub fn unlink<P>(&mut self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        if let Some(key) = self.paths.remove(path.as_ref()) {
            self.refcounts[key] -= 1;
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::NotFound,
                format!("'{}' not found", path.as_ref().display()),
            ))
        }
    }

    pub fn get<P>(&self, path: P) -> Result<&Entry>
    where
        P: AsRef<Path>,
    {
        self.paths
            .get(path.as_ref())
            .and_then(|key| self.inodes.get(*key))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("'{}' not found", path.as_ref().display()),
                )
            })
    }

    pub fn get_mut<P>(&mut self, path: P) -> Result<&mut Entry>
    where
        P: AsRef<Path>,
    {
        self.paths
            .get(path.as_ref())
            .and_then(|key| self.inodes.get_mut(*key))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("'{}' not found", path.as_ref().display()),
                )
            })
    }

    pub fn get_file<P>(&self, path: P) -> Result<&File>
    where
        P: AsRef<Path>,
    {
        match self.get(path.as_ref())? {
            Entry::File(f) => Ok(f),
            _ => Err(Error::other(format!(
                "'{}' is not a file",
                path.as_ref().display()
            ))),
        }
    }

    pub fn get_file_mut<P>(&mut self, path: P) -> Result<&mut File>
    where
        P: AsRef<Path>,
    {
        match self.get_mut(path.as_ref())? {
            Entry::File(f) => Ok(f),
            _ => Err(Error::other(format!(
                "'{}' is not a file",
                path.as_ref().display()
            ))),
        }
    }

    pub fn chmod<P>(&mut self, path: P, mode: Mode) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.get_mut(path)?.chmod(mode);
        Ok(())
    }

    pub fn chown<P>(&mut self, path: P, uid: Uid, gid: Gid) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.get_mut(path)?.chown(uid, gid);
        Ok(())
    }

    pub fn rename<P1, P2>(&mut self, from: P1, to: P2) -> Result<()>
    where
        P1: AsRef<Path>,
        P2: Into<BytesPath>,
    {
        let inode = self.paths.remove(from.as_ref()).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("'{}' not found", from.as_ref().display()),
            )
        })?;
        let to = to.into();
        if self.paths.contains_key(&to) {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!("'{}' already exists", to.display()),
            ));
        }
        self.paths.insert(to, inode);
        Ok(())
    }

    pub fn set_times<P>(
        &mut self,
        path: P,
        created: SystemTime,
        accessed: SystemTime,
        modified: SystemTime,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.get_mut(path)?
            .metadata_mut()
            .set_times(created, accessed, modified);
        Ok(())
    }

    /// Create a hard link to an existing file. This increments the refcount of
    /// the original inode. The the original path is later unlinked, this
    /// reference will keep the underlying entry alive.
    pub fn link<P1, P2>(&mut self, old: P1, new: P2) -> Result<()>
    where
        P1: AsRef<Path>,
        P2: Into<BytesPath>,
    {
        let key = self.paths.get(old.as_ref()).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("'{}' not found", old.as_ref().display()),
            )
        })?;
        if !self.inodes[*key].is_directory() {
            return Err(Error::new(
                ErrorKind::IsADirectory,
                "directory cannot be hardlink target",
            ));
        }
        self.refcounts
            .entry(*key)
            .expect("refcount impossibly None")
            .and_modify(|r| *r += 1);
        self.paths.insert(new.into(), *key);
        Ok(())
    }

    pub fn truncate<P>(&mut self, path: P, len: u64) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.get_file_mut(path)?.truncate(len);
        Ok(())
    }

    /// Remove a directory, failing if it is not empty
    pub fn rmdir<P>(&mut self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let dir = path.as_ref();
        if !self.get(dir)?.is_directory() {
            return Err(Error::new(
                ErrorKind::NotADirectory,
                format!("'{}' is not a directory", dir.display()),
            ));
        }
        if self
            .paths
            .iter()
            .any(|(p, _)| p.starts_with(dir) && dir != p.as_path())
        {
            return Err(Error::new(
                ErrorKind::DirectoryNotEmpty,
                format!("'{}' is not empty", dir.display()),
            ));
        }
        Ok(())
    }
}

impl Default for Filesystem {
    fn default() -> Self {
        Self::new()
    }
}

// Exact inode number equality is unimportant, what is important is that all the
// visible attributes of a filesystem are equal (in other words, data that is
// accessible via a path)
impl PartialEq<Filesystem> for Filesystem {
    fn eq(&self, other: &Self) -> bool {
        let mut unvisited: HashSet<&Path> = other.paths.keys().map(|k| k.as_path()).collect();
        for (path, entry) in self {
            unvisited.remove(path);
            if let Ok(other) = other.get(path) {
                if entry != other {
                    return false;
                }
            } else {
                return false;
            }
        }
        unvisited.is_empty()
    }
}

impl Eq for Filesystem {}

impl cmp::ApproxEq for Filesystem {
    #[deny(unused_variables)]
    fn cmp(&self, other: &Self) -> cmp::Fields {
        let Self {
            paths,
            inodes,
            refcounts: _,
        } = &self;
        let mut f = cmp::Fields::all();
        #[allow(clippy::mutable_key_type)]
        let self_paths: HashSet<_> = paths.keys().collect();
        #[allow(clippy::mutable_key_type)]
        let other_paths: HashSet<_> = other.paths.keys().collect();
        if self_paths != other_paths {
            f.remove(cmp::Fields::PATH);
        }
        for (path, inode) in paths {
            let entry = &inodes[*inode];
            match other.get(path) {
                Err(_) => f.remove(cmp::Fields::all_entry_fields()),
                Ok(other_entry) => {
                    f = f.intersection(cmp::ApproxEq::cmp(entry, other_entry));
                }
            }
        }
        f
    }
}

impl<P> FromIterator<(P, Entry)> for Filesystem
where
    P: Into<BytesPath>,
{
    fn from_iter<T: IntoIterator<Item = (P, Entry)>>(iter: T) -> Self {
        let mut fs = Self::new();
        for (path, entry) in iter {
            fs.insert(path, entry);
        }
        fs
    }
}

impl<P, const N: usize> From<[(P, Entry); N]> for Filesystem
where
    P: Into<BytesPath>,
{
    fn from(value: [(P, Entry); N]) -> Self {
        value.into_iter().collect()
    }
}

impl Debug for Filesystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_map();
        for (path, entry) in self {
            d.entry(&path, entry);
        }
        d.finish()
    }
}

macro_rules! id_type {
    ($i:ident, $nix:ty) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $i(u32);

        impl $i {
            pub fn from_raw(id: u32) -> Self {
                Self(id)
            }

            pub fn as_u32(&self) -> u32 {
                self.0
            }
        }

        impl From<u32> for $i {
            fn from(id: u32) -> Self {
                Self(id)
            }
        }

        impl From<$nix> for $i {
            fn from(id: $nix) -> Self {
                Self(id.as_raw())
            }
        }

        impl AsRef<u32> for $i {
            fn as_ref(&self) -> &u32 {
                self
            }
        }

        impl std::ops::Deref for $i {
            type Target = u32;

            fn deref(&self) -> &u32 {
                &self.0
            }
        }

        impl std::fmt::Debug for $i {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($i), self.0)
            }
        }
    };
}

id_type!(Uid, nix::unistd::Uid);
id_type!(Gid, nix::unistd::Gid);

mod __private {
    pub trait Sealed {}
}

#[cfg(test)]
pub(crate) mod tests {
    use nix::sys::stat::Mode;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::entry::Directory;
    use crate::entry::Metadata;
    use crate::entry::Symlink;

    /// Standard demo filesystem to exercise a variety of formats.
    pub(crate) fn demo_fs() -> Filesystem {
        Filesystem::from([
            (
                "",
                Directory::builder()
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o755))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .build(),
                    )
                    .build()
                    .into(),
            ),
            (
                "testdata",
                Directory::builder()
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o755))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .build(),
                    )
                    .build()
                    .into(),
            ),
            (
                "testdata/lorem.txt",
                File::builder()
                    .contents("Lorem ipsum\n")
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .xattr("user.demo", "lorem ipsum")
                            .build(),
                    )
                    .build()
                    .into(),
            ),
            (
                "testdata/dir",
                Directory::builder()
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o755))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .build(),
                    )
                    .build()
                    .into(),
            ),
            (
                "testdata/dir/lorem.txt",
                File::builder()
                    .contents("Lorem ipsum dolor sit amet\n")
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .build(),
                    )
                    .build()
                    .into(),
            ),
            (
                "testdata/dir/symlink",
                Symlink::new(
                    "../lorem.txt",
                    Some(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o777))
                            .uid(Uid::from_raw(0))
                            .gid(Gid::from_raw(0))
                            .build(),
                    ),
                )
                .into(),
            ),
        ])
    }

    #[test]
    fn partial_eq() {
        assert_eq!(demo_fs(), demo_fs());
        let mut other = demo_fs().clone();
        other.unlink("testdata/dir/lorem.txt").unwrap();
        assert_ne!(demo_fs(), other);
    }
}
