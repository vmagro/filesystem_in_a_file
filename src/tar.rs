use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::Cursor;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use memmap::Mmap;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use tar::Archive;
use tar::EntryType;

use crate::entry::Directory;
use crate::File;
use crate::Filesystem;
use crate::__private::Sealed;
use crate::extract::ReflinkExtract;

pub trait Backing: Sealed {}

impl Sealed for std::fs::File {}
impl Backing for std::fs::File {}

pub struct Tar<B: Backing> {
    contents: Mmap,
    backing: B,
}

impl Tar<std::fs::File> {
    /// Load an uncompressed tarball from a [std::fs::File].
    pub fn from_file(file: std::fs::File) -> std::io::Result<Self> {
        let contents = unsafe { memmap::MmapOptions::new().map(&file) }?;
        Ok(Self {
            contents,
            backing: file,
        })
    }
}

impl ReflinkExtract for Tar<std::fs::File> {
    fn reflink_extract(&self, dir: &Path) -> std::io::Result<()> {
        let fs = self.filesystem()?;
        fs.reflink_extract(dir, &self.backing, self.contents.as_ptr())
    }
}

impl<B: Backing> Tar<B> {
    pub fn filesystem(&self) -> std::io::Result<Filesystem<'_, '_>> {
        let mut fs = Filesystem::new();
        for entry in Archive::new(Cursor::new(&self.contents)).entries_with_seek()? {
            let mut entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let path = Cow::Owned(entry.path()?.to_path_buf());
            let mut xattrs = BTreeMap::new();
            if let Ok(Some(pax_extensions)) = entry.pax_extensions() {
                for ext in pax_extensions.into_iter().filter_map(Result::ok) {
                    if ext.key_bytes().starts_with(b"SCHILY.xattr.") {
                        xattrs.insert(
                            OsString::from_vec(ext.key_bytes()["SCHILY.xattr.".len()..].to_vec()),
                            ext.value_bytes().to_vec(),
                        );
                    }
                }
            }
            let xattrs = xattrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect::<BTreeMap<Cow<'_, OsStr>, Cow<'_, [u8]>>>();
            if entry.header().entry_type() == EntryType::Directory {
                fs.entries.insert(
                    path,
                    Directory::builder()
                        .mode(Mode::from_bits_truncate(entry.header().mode()?))
                        .uid(Uid::from_raw(entry.header().uid()? as u32))
                        .gid(Gid::from_raw(entry.header().gid()? as u32))
                        .xattrs(xattrs)
                        .build()
                        .into(),
                );
            } else if entry.header().entry_type() == EntryType::Regular {
                fs.entries.insert(
                    path,
                    File::builder()
                        .contents(&self.contents[file_offset..file_offset + entry.size() as usize])
                        .mode(Mode::from_bits_truncate(entry.header().mode()?))
                        .uid(Uid::from_raw(entry.header().uid()? as u32))
                        .gid(Gid::from_raw(entry.header().gid()? as u32))
                        .xattrs(xattrs)
                        .build()
                        .into(),
                );
            }
        }
        Ok(fs)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn tar() {
        let file = std::fs::File::open(
            Path::new(env!("OUT_DIR")).join("testdata.tar"),
        )
        .expect("failed to open testdata.tar");
        let testdata_tar = Tar::from_file(file).expect("failed to load tar");
        let fs = testdata_tar.filesystem().expect("failed to parse tar");
        let mut demo_fs = demo_fs();
        // tar is missing the top-level directory
        demo_fs.entries.remove(Path::new(""));
        assert_eq!(fs, demo_fs);
    }

    #[test]
    fn reflink_extract() {
        let file = std::fs::File::open(
            Path::new(env!("OUT_DIR")).join("testdata.tar"),
        )
        .expect("failed to open testdata.tar");
        let testdata_tar = Tar::from_file(file).expect("failed to load tar");

        let tmpdir = tempfile::TempDir::new_in(Path::new(env!("OUT_DIR")))
            .expect("failed to create tmpdir");

        testdata_tar
            .reflink_extract(tmpdir.path())
            .expect("failed to extract");

        let extracted_fs =
            Filesystem::from_dir(tmpdir.path()).expect("failed to read extracted dir");
        assert_eq!(extracted_fs, demo_fs());
    }
}
