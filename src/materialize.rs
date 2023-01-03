use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use nix::sys::stat::SFlag;

use crate::Entry;
use crate::Filesystem;

impl Filesystem {
    /// Materialize the in-memory representation of this [Filesystem] to a real
    /// on-disk filesystem.
    pub fn materialize_to(&self, dir: &Path) -> std::io::Result<()> {
        for (path, entry) in self {
            let dst_path = dir.join(path);
            #[remain::sorted]
            match entry {
                Entry::Directory(_) => {
                    // Do not create top-level directory, but still let the
                    // later chown+chmod happen.
                    if path != Path::new("") {
                        std::fs::create_dir(&dst_path)?;
                    }
                }
                Entry::File(f) => {
                    let mut dst_f = std::fs::File::create(&dst_path)?;
                    dst_f.write_all(&f.to_bytes())?;
                }
                Entry::Special(s) => {
                    if s.file_type().contains(SFlag::S_IFIFO) {
                        nix::unistd::mkfifo(&dst_path, s.metadata().mode)?;
                    } else {
                        todo!("{s:?}");
                    }
                }
                Entry::Symlink(s) => {
                    std::os::unix::fs::symlink(s.target(), &dst_path)?;
                    std::os::unix::fs::lchown(
                        &dst_path,
                        Some(entry.metadata().uid().into()),
                        Some(entry.metadata().gid().into()),
                    )?;
                }
            }
            if !matches!(entry, Entry::Symlink(_)) {
                std::fs::set_permissions(&dst_path, entry.metadata().permissions())?;
                nix::unistd::chown(
                    &dst_path,
                    Some(entry.metadata().uid()),
                    Some(entry.metadata().gid()),
                )?;
                for (name, val) in entry.metadata().xattrs() {
                    let name = OsStr::from_bytes(&name);
                    xattr::set(&dst_path, name, val)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dir")]
    use super::*;

    #[cfg(feature = "dir")]
    #[test]
    fn matches_demo() {
        // Ensure that the tmpdir is in the same filesystem as the source repo.
        // This has two main purposes:
        // - ensure xattrs are supported
        // - make reflink copies work
        let tmpdir = tempfile::TempDir::new_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("failed to create tmpdir");
        let demo_fs = crate::tests::demo_fs();
        demo_fs
            .materialize_to(tmpdir.path())
            .expect("failed to materialize");
        let materialized_fs =
            Filesystem::from_dir(tmpdir.path()).expect("failed to read materialized dir");
        crate::cmp::assert_approx_eq!(
            materialized_fs,
            demo_fs,
            crate::cmp::Fields::all() - crate::cmp::Fields::TIME
        );
    }
}
