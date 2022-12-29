use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use bytes::Bytes;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use walkdir::WalkDir;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::entry::Symlink;
use crate::File;
use crate::Filesystem;

impl Filesystem {
    /// Create an in-memory view of a Filesystem from a directory on a real,
    /// on-disk filesystem.
    pub fn from_dir(path: &Path) -> std::io::Result<Self> {
        let mut fs = Self::new();
        for entry in WalkDir::new(path) {
            let entry = entry?;
            let relpath = entry
                .path()
                .strip_prefix(path)
                .expect("symlink traversal is disabled, this path must be under the top directory")
                .to_path_buf();
            let meta = entry.metadata()?;
            let mut xattrs = BTreeMap::new();
            for name in xattr::list(entry.path())? {
                let val = xattr::get(entry.path(), &name)?.expect("must exist");
                xattrs.insert(name, val);
            }
            let xattrs: BTreeMap<Bytes, Bytes> = xattrs
                .into_iter()
                .map(|(k, v)| (Bytes::copy_from_slice(k.as_bytes()), v.into()))
                .collect();
            if entry.file_type().is_dir() {
                fs.insert(
                    relpath,
                    Directory::builder()
                        .metadata(
                            Metadata::builder()
                                .mode(Mode::from_bits_truncate(meta.mode()))
                                .uid(Uid::from_raw(meta.uid()))
                                .gid(Gid::from_raw(meta.gid()))
                                .xattrs(xattrs)
                                .build(),
                        )
                        .build(),
                );
            } else if entry.file_type().is_symlink() {
                let target = std::fs::read_link(entry.path())?;
                let symlink_meta = std::fs::symlink_metadata(entry.path())?;
                fs.insert(relpath, Symlink::new(target, Some(symlink_meta.into())));
            } else if entry.file_type().is_file() {
                fs.insert(
                    relpath,
                    File::builder()
                        .contents(std::fs::read(entry.path())?)
                        .metadata(
                            Metadata::builder()
                                .mode(Mode::from_bits_truncate(meta.mode()))
                                .uid(Uid::from_raw(meta.uid()))
                                .gid(Gid::from_raw(meta.gid()))
                                .xattrs(xattrs)
                                .build(),
                        )
                        .build(),
                );
            }
        }
        Ok(fs)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn from_dir() {
        let fs = Filesystem::from_dir(&Path::new(env!("OUT_DIR")).join("fs"))
            .expect("failed to load from directory");
        assert_eq!(demo_fs(), fs);
    }
}
