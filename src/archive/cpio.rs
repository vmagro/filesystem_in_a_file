use std::io::Cursor;
use std::path::Path;

use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::File;
use crate::Filesystem;

const HEADER_LEN: usize = 110;

impl<'f> Filesystem<'f> {
    /// Parse an uncompressed cpio
    pub fn parse_cpio(contents: &'f [u8]) -> std::io::Result<Self> {
        let mut fs = Self::new();
        let mut cursor = Cursor::new(&contents);
        let mut header_start_pos = 0;
        loop {
            let reader = cpio::newc::Reader::new(cursor).expect("failed to create reader");
            let entry = reader.entry();
            if entry.is_trailer() {
                break;
            }
            // let path = Path::new(entry.name());
            let path = Path::new("/a");
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
                let contents = &contents[file_start..file_start + file_size];
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
    use memmap::MmapOptions;

    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn cpio() {
        let file = std::fs::File::open(Path::new(env!("OUT_DIR")).join("testdata.cpio"))
            .expect("failed to open testdata.cpio");
        let contents = unsafe { MmapOptions::new().map(&file).unwrap() };
        let fs = Filesystem::parse_cpio(&contents).expect("failed to parse cpio");
        let mut demo_fs = demo_fs();
        // cpio is missing the top-level directory
        demo_fs.entries.remove(Path::new(""));
        // cpio does not support xattrs
        demo_fs
            .entries
            .values_mut()
            .for_each(|ent| ent.metadata_mut().clear_xattrs());
        assert_eq!(demo_fs, fs);
    }
}
