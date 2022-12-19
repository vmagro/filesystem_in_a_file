//! In-memory representation of a filesystem designed to enable full-filesystem
//! comparisons during integration tests.
//! Great care is taken to ensure that all the structs in this crate are
//! zero-copy, allowing a user to read (or better yet, mmap) a
//! filesystem-in-a-file (such as a tarball, BTRFS sendstream, cpio archive,
//! etc) and get a complete picture of the entire FS (or at least the parts that
//! can be represented in the archive format).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

mod dir;
mod entry;
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

    /// Extract the in-memory representation of this [Filesystem] to a real
    /// on-disk filesystem.
    pub fn extract_to(&self, dir: &Path) -> std::io::Result<()> {
        self.extract_to_internal(dir, None)
    }

    /// See [Filesystem::extract_to].
    /// By tracking the backing [std::fs::File], the extract implementation can
    /// be more efficient by using copy_file_range. Because the Rust
    /// implementation of [std::io::copy] is sealed to std-only types, we need
    /// the caller to provide the backing file.
    pub fn extract_with_backing_file_to(
        &self,
        backing_file: &std::fs::File,
        dir: &Path,
    ) -> std::io::Result<()> {
        self.extract_to_internal(dir, Some(backing_file))
    }

    fn extract_to_internal(
        &self,
        dir: &Path,
        backing_file: Option<&std::fs::File>,
    ) -> std::io::Result<()> {
        for (path, entry) in &self.entries {
            let dst_path = dir.join(path);
            match entry {
                Entry::Directory(_) => {
                    std::fs::create_dir(&dst_path)?;
                }
                Entry::File(f) => {
                    let mut dst_f = std::fs::File::create(&dst_path)?;
                    // TODO: use copy_file_range when backing_file is provided
                    dst_f.write_all(&f.to_bytes())?;
                }
            }
            std::fs::set_permissions(&dst_path, entry.permissions())?;
            nix::unistd::chown(&dst_path, Some(entry.uid()), Some(entry.gid()))?;
            for (name, val) in entry.xattrs() {
                xattr::set(&dst_path, name, val)?;
            }
        }
        Ok(())
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
