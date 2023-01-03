//! In-memory representation of a filesystem designed to enable full-filesystem
//! comparisons during integration tests.
//! Great care is taken to ensure that all the structs in this crate are
//! zero-copy, allowing a user to read (or better yet, mmap) a
//! filesystem-in-a-file (such as a tarball, BTRFS sendstream, cpio archive,
//! etc) and get a complete picture of the entire FS (or at least the parts that
//! can be represented in the archive format).

#![feature(io_error_other)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(unix_chown)]

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::path::Path;
use std::time::SystemTime;

use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use slotmap::SecondaryMap;
use slotmap::SlotMap;

#[cfg(feature = "archive")]
pub mod archive;
#[cfg(feature = "btrfs")]
pub mod btrfs;
mod bytes_ext;
pub mod cmp;
#[cfg(feature = "dir")]
mod dir;
mod entry;
pub mod file;
mod iter;
mod materialize;
mod path;

pub(crate) use bytes_ext::BytesExt;
pub use entry::Entry;
use file::File;
pub use path::BytesPath;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("entry does not exist")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;

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

    pub fn unlink<P>(&mut self, path: P) -> bool
    where
        P: AsRef<Path>,
    {
        if let Some(key) = self.paths.remove(path.as_ref()) {
            self.refcounts[key] -= 1;
            true
        } else {
            false
        }
    }

    pub fn get<P>(&self, path: P) -> Result<&Entry>
    where
        P: AsRef<Path>,
    {
        self.paths
            .get(path.as_ref())
            .and_then(|key| self.inodes.get(*key))
            .ok_or(Error::NotFound)
    }

    pub fn get_mut<P>(&mut self, path: P) -> Result<&mut Entry>
    where
        P: AsRef<Path>,
    {
        self.paths
            .get(path.as_ref())
            .and_then(|key| self.inodes.get_mut(*key))
            .ok_or(Error::NotFound)
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

    pub fn rename<P>(&mut self, from: P, to: impl Into<BytesPath>) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let inode = self.paths.remove(from.as_ref()).ok_or(Error::NotFound)?;
        self.paths.insert(to.into(), inode);
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
        let self_paths: HashSet<_> = paths.keys().collect();
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

mod __private {
    pub trait Sealed {}
}

#[cfg(test)]
pub(crate) mod tests {
    use nix::sys::stat::Mode;
    use nix::unistd::Gid;
    use nix::unistd::Uid;
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
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
        other.unlink("testdata/dir/lorem.txt");
        assert_ne!(demo_fs(), other);
    }
}
