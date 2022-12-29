//! In-memory representation of a filesystem designed to enable full-filesystem
//! comparisons during integration tests.
//! Great care is taken to ensure that all the structs in this crate are
//! zero-copy, allowing a user to read (or better yet, mmap) a
//! filesystem-in-a-file (such as a tarball, BTRFS sendstream, cpio archive,
//! etc) and get a complete picture of the entire FS (or at least the parts that
//! can be represented in the archive format).

// both of these features are now stabilized in 1.66
#![feature(map_first_last)]
#![feature(mixed_integer_ops)]
#![feature(io_error_other)]
#![feature(unix_chown)]

use std::collections::BTreeMap;

#[cfg(feature = "archive")]
pub mod archive;
#[cfg(feature = "btrfs")]
pub mod btrfs;
mod bytes_ext;
#[cfg(feature = "dir")]
mod dir;
mod entry;
mod extract;
pub mod file;
mod path;

pub(crate) use bytes_ext::BytesExt;
pub use entry::Entry;
use file::File;
pub use path::BytesPath;

/// Full view of a filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filesystem<'f> {
    entries: BTreeMap<BytesPath, Entry<'f>>,
}

impl<'f> Filesystem<'f> {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
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

    use super::*;
    use crate::entry::Directory;
    use crate::entry::Metadata;
    use crate::entry::Symlink;

    /// Standard demo filesystem to exercise a variety of formats.
    pub(crate) fn demo_fs() -> Filesystem<'static> {
        Filesystem {
            entries: BTreeMap::from([
                (
                    "".into(),
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
                    "testdata".into(),
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
                    "testdata/lorem.txt".into(),
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
                    "testdata/dir".into(),
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
                    "testdata/dir/lorem.txt".into(),
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
                    "testdata/dir/symlink".into(),
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
            ]),
        }
    }
}
