use std::borrow::Cow;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;

use memmap::Mmap;
use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::File;
use crate::Filesystem;
use crate::__private::Sealed;
use crate::extract::ReflinkExtract;

const HEADER_LEN: usize = 110;

pub trait Backing: Sealed {}

impl Backing for std::fs::File {}

pub struct Cpio<B: Backing> {
    contents: Mmap,
    backing: B,
}

impl Cpio<std::fs::File> {
    /// Load an uncompressed cpio from a [std::fs::File].
    pub fn from_file(file: std::fs::File) -> std::io::Result<Self> {
        let contents = unsafe { memmap::MmapOptions::new().map(&file) }?;
        Ok(Self {
            contents,
            backing: file,
        })
    }
}

impl ReflinkExtract for Cpio<std::fs::File> {
    fn reflink_extract(&self, dir: &Path) -> std::io::Result<()> {
        let fs = self.filesystem()?;
        fs.reflink_extract(dir, &self.backing, self.contents.as_ptr())
    }
}

impl<B: Backing> Cpio<B> {
    pub fn filesystem(&self) -> std::io::Result<Filesystem<'_, '_>> {
        let mut fs = Filesystem::new();
        let mut cursor = Cursor::new(&self.contents);
        let mut header_start_pos = 0;
        loop {
            let reader = cpio::newc::Reader::new(cursor).expect("failed to create reader");
            let entry = reader.entry();
            if entry.is_trailer() {
                break;
            }
            let path = Cow::Owned(PathBuf::from(entry.name()));
            let mode = Mode::from_bits_truncate(entry.mode());
            let sflag = SFlag::from_bits_truncate(entry.mode());
            if sflag.contains(SFlag::S_IFDIR) {
                fs.entries.insert(
                    path,
                    Directory::builder()
                        .metadata(
                            Metadata::builder()
                                .mode(mode)
                                .uid(Uid::from_raw(entry.uid()))
                                .gid(Gid::from_raw(entry.gid()))
                                .build(),
                        )
                        .build()
                        .into(),
                );
            } else if sflag.contains(SFlag::S_IFREG) {
                let mut builder = File::builder();
                builder.metadata(
                    Metadata::builder()
                        .mode(mode)
                        .uid(Uid::from_raw(entry.uid()))
                        .gid(Gid::from_raw(entry.gid()))
                        .build(),
                );
                let file_size = entry.file_size() as usize;
                // the file starts at the header_start + HEADER_LEN + path, padded to
                // the next multiple of 4, then 4 bytes after that
                let file_start =
                    ((header_start_pos + HEADER_LEN + entry.name().len() + 3) & !3) + 4;
                let contents = &self.contents[file_start..file_start + file_size];
                builder.contents(contents);
                fs.entries.insert(path, builder.build().into());
            } else {
                todo!();
            }
            cursor = reader.finish().expect("finish failed");
            header_start_pos = cursor.position() as usize;
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
    fn cpio() {
        let file = std::fs::File::open(Path::new(env!("OUT_DIR")).join("testdata.cpio"))
            .expect("failed to open testdata.cpio");
        let testdata_cpio = Cpio::from_file(file).expect("failed to load cpio");
        let fs = testdata_cpio.filesystem().expect("failed to parse cpio");
        let mut demo_fs = demo_fs();
        // cpio is missing the top-level directory
        demo_fs.entries.remove(Path::new(""));
        // cpio does not support xattrs
        demo_fs
            .entries
            .values_mut()
            .for_each(|ent| ent.metadata_mut().clear_xattrs());
        assert_eq!(fs, demo_fs);
    }

    #[test]
    fn reflink_extract() {
        let file = std::fs::File::open(Path::new(env!("OUT_DIR")).join("testdata.cpio"))
            .expect("failed to open testdata.cpio");
        let testdata_cpio = Cpio::from_file(file).expect("failed to load cpio");

        let tmpdir =
            tempfile::TempDir::new_in(Path::new(env!("OUT_DIR"))).expect("failed to create tmpdir");

        testdata_cpio
            .reflink_extract(tmpdir.path())
            .expect("failed to extract");

        let extracted_fs =
            Filesystem::from_dir(tmpdir.path()).expect("failed to read extracted dir");
        let mut demo_fs = demo_fs();
        // cpio does not support xattrs
        demo_fs
            .entries
            .values_mut()
            .for_each(|ent| ent.metadata_mut().clear_xattrs());
        assert_eq!(extracted_fs, demo_fs);
    }
}
