use std::os::unix::fs::MetadataExt;
use std::path::Path;

use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use walkdir::WalkDir;

use crate::entry::Directory;
use crate::File;
use crate::Filesystem;

impl<'f> Filesystem<'f, 'f> {
    /// Create an in-memory view of a Filesystem from a directory on a real,
    /// on-disk filesystem.
    pub fn from_dir(path: &Path) -> std::io::Result<Self> {
        let mut fs = Self::new();
        for entry in WalkDir::new(path) {
            let entry = entry?;
            eprintln!(
                "{:?} {:?} {}",
                entry.path(),
                entry.file_type(),
                entry.file_type().is_dir()
            );
            let relpath = entry
                .path()
                .strip_prefix(path)
                .expect("symlink traversal is disabled, this path must be under the top directory")
                .to_path_buf();
            let meta = entry.metadata()?;
            if entry.file_type().is_dir() {
                let mut builder = Directory::builder();
                builder
                    .mode(Mode::from_bits_truncate(meta.mode()))
                    .uid(Uid::from_raw(meta.uid()))
                    .gid(Gid::from_raw(meta.gid()));
                for name in xattr::list(entry.path())? {
                    let val = xattr::get(entry.path(), &name)?.expect("must exist");
                    builder.xattr(name, val);
                }
                fs.entries.insert(relpath.into(), builder.build().into());
            } else if entry.file_type().is_symlink() {
                todo!()
            } else if entry.file_type().is_file() {
                let contents = std::fs::read(entry.path())?;
                let mut builder = File::builder();
                builder
                    .contents(contents)
                    .mode(Mode::from_bits_truncate(meta.mode()))
                    .uid(Uid::from_raw(meta.uid()))
                    .gid(Gid::from_raw(meta.gid()));
                for name in xattr::list(entry.path())? {
                    let val = xattr::get(entry.path(), &name)?.expect("must exist");
                    builder.xattr(name, val);
                }
                fs.entries.insert(relpath.into(), builder.build().into());
            }
        }
        Ok(fs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn from_dir() {
        let fs =
            Filesystem::from_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tar/testdata"))
                .expect("failed to load from directory");
        assert_eq!(
            fs,
            Filesystem {
                entries: BTreeMap::from([
                    (
                        Path::new("").into(),
                        Directory::builder()
                            .mode(Mode::from_bits_truncate(0o755))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .build()
                            .into()
                    ),
                    (
                        Path::new("lorem.txt").into(),
                        File::builder()
                            .contents(b"Lorem ipsum\n")
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .xattr(OsStr::new("user.demo"), &b"lorem ipsum"[..])
                            .build()
                            .into()
                    ),
                    (
                        Path::new("dir").into(),
                        Directory::builder()
                            .mode(Mode::from_bits_truncate(0o755))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .build()
                            .into()
                    ),
                    (
                        Path::new("dir/lorem.txt").into(),
                        File::builder()
                            .contents(b"Lorem ipsum dolor sit amet\n")
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .build()
                            .into()
                    ),
                ]),
            }
        );
    }
}
