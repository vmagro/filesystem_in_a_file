use std::io::Write;
use std::path::Path;

use crate::Entry;
use crate::Filesystem;

impl<'p, 'f> Filesystem<'p, 'f> {
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
                    // Do not create top-level directory, but still let the
                    // later chown+chmod happen.
                    if path != Path::new("") {
                        std::fs::create_dir(&dst_path)?;
                    }
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
mod tests {
    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn extract_matches_demo() {
        // Ensure that the tmpdir is in the same filesystem as the source repo.
        // This has two main purposes:
        // - ensure xattrs are supported
        // - make reflink copies work
        let tmpdir = tempfile::TempDir::new_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("failed to create tmpdir");
        let demo_fs = demo_fs();
        demo_fs
            .extract_to(tmpdir.path())
            .expect("failed to extract");
        let extracted_fs =
            Filesystem::from_dir(tmpdir.path()).expect("failed to read extracted dir");
        assert_eq!(extracted_fs, demo_fs);
    }
}
