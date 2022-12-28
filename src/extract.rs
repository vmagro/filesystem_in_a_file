use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use nix::fcntl::copy_file_range;

use crate::Entry;
use crate::Filesystem;

struct ReflinkInfo<'a> {
    base_ptr: *const u8,
    backing_file: &'a std::fs::File,
}

/// Trait indicating support for efficient reflink-based extraction of a
/// filesystem-in-a-file.
pub trait ReflinkExtract {
    fn reflink_extract(&self, dir: &Path) -> std::io::Result<()>;
}

impl<'f> Filesystem<'f> {
    /// Extract the in-memory representation of this [Filesystem] to a real
    /// on-disk filesystem.
    pub fn extract_to(&self, dir: &Path) -> std::io::Result<()> {
        self.extract_to_internal(dir, None)
    }

    /// See [Filesystem::extract_to].
    /// By tracking the backing [std::fs::File], the extract implementation can
    /// be more efficient by using copy_file_range.
    pub(crate) fn reflink_extract(
        &self,
        dir: &Path,
        backing_file: &std::fs::File,
        base_ptr: *const u8,
    ) -> std::io::Result<()> {
        self.extract_to_internal(
            dir,
            Some(ReflinkInfo {
                base_ptr,
                backing_file,
            }),
        )
    }

    fn extract_to_internal(
        &self,
        dir: &Path,
        mut reflink_info: Option<ReflinkInfo<'_>>,
    ) -> std::io::Result<()> {
        for (path, entry) in &self.entries {
            let dst_path = dir.join(path);
            match entry {
                Entry::Directory(_) => {
                    // Do not create top-level directory, but still let the
                    // later chown+chmod happen.
                    if *path != Path::new("") {
                        std::fs::create_dir(&dst_path)?;
                    }
                }
                Entry::File(f) => {
                    let mut dst_f = std::fs::File::create(&dst_path)?;
                    if let Some(reflink_info) = reflink_info.as_mut() {
                        for extent in f.extents.values() {
                            let offset_into_file = unsafe {
                                extent.data().as_ptr().offset_from(reflink_info.base_ptr)
                            };
                            assert!(
                                offset_into_file > 0,
                                "offset_into_file should be positive if base_ptr is correct"
                            );
                            reflink_info
                                .backing_file
                                .seek(SeekFrom::Start(offset_into_file as u64))?;
                            copy_file_range(
                                reflink_info.backing_file.as_raw_fd(),
                                None,
                                dst_f.as_raw_fd(),
                                None,
                                extent.len() as usize,
                            )?;
                        }
                    } else {
                        dst_f.write_all(&f.to_bytes())?;
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
                    xattr::set(&dst_path, name, val)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::demo_fs;

    #[cfg(feature = "dir")]
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
