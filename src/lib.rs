//! In-memory representation of a filesystem designed to enable full-filesystem
//! comparisons during integration tests.
//! Great care is taken to ensure that all the structs in this crate are
//! zero-copy, allowing a user to read (or better yet, mmap) a
//! filesystem-in-a-file (such as a tarball, BTRFS sendstream, cpio archive,
//! etc) and get a complete picture of the entire FS (or at least the parts that
//! can be represented in the archive format).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

mod dir;
mod entry;
mod extract;
pub mod file;
#[cfg(feature = "tar")]
mod tar;

pub use entry::Entry;
use file::File;

/// Full view of a filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filesystem<'p, 'f> {
    entries: BTreeMap<Cow<'p, Path>, Entry<'f>>,
}

impl<'p, 'f> Filesystem<'p, 'f> {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::ffi::OsStr;

    use nix::sys::stat::Mode;
    use nix::unistd::Gid;
    use nix::unistd::Uid;

    use super::*;
    use crate::entry::Directory;

    /// Standard demo filesystem to exercise a variety of formats.
    pub(crate) fn demo_fs() -> Filesystem<'static, 'static> {
        Filesystem {
            entries: BTreeMap::from([
                (
                    Path::new("").into(),
                    Directory::builder()
                        .mode(Mode::from_bits_truncate(0o755))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .build()
                        .into(),
                ),
                (
                    Path::new("testdata").into(),
                    Directory::builder()
                        .mode(Mode::from_bits_truncate(0o755))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .build()
                        .into(),
                ),
                (
                    Path::new("testdata/lorem.txt").into(),
                    File::builder()
                        .contents(b"Lorem ipsum\n")
                        .mode(Mode::from_bits_truncate(0o644))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .xattr(OsStr::new("user.demo"), &b"lorem ipsum"[..])
                        .build()
                        .into(),
                ),
                (
                    Path::new("testdata/dir").into(),
                    Directory::builder()
                        .mode(Mode::from_bits_truncate(0o755))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .build()
                        .into(),
                ),
                (
                    Path::new("testdata/dir/lorem.txt").into(),
                    File::builder()
                        .contents(b"Lorem ipsum dolor sit amet\n")
                        .mode(Mode::from_bits_truncate(0o644))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .build()
                        .into(),
                ),
            ]),
        }
    }
}
