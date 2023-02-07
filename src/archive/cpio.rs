use std::io::Cursor;

use bytes::Bytes;
use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::unistd::Gid;
use nix::unistd::Uid;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::entry::Symlink;
use crate::BytesExt;
use crate::File;
use crate::Filesystem;

const HEADER_LEN: usize = 110;

// Good description of the cpio format can be found here
// https://www.kernel.org/doc/Documentation/early-userspace/buffer-format.txt

fn align_to_4_bytes(pos: usize) -> usize {
    let remainder = pos % 4;
    if remainder != 0 {
        pos + (4 - remainder)
    } else {
        pos
    }
}

impl Filesystem {
    /// Parse an uncompressed cpio
    pub fn parse_cpio(contents: &Bytes) -> std::io::Result<Self> {
        let mut fs = Self::new();
        let mut cursor = Cursor::new(&contents);

        let mut header_start_pos = 0;
        loop {
            let reader = cpio::newc::Reader::new(cursor).expect("failed to create reader");
            let entry = reader.entry();
            if entry.is_trailer() {
                break;
            }
            let path = contents.subslice_or_copy(entry.name().as_bytes());
            let mode = Mode::from_bits_truncate(entry.mode());
            let sflag = SFlag::from_bits_truncate(entry.mode());
            let metadata = Metadata::builder()
                .mode(mode)
                .uid(Uid::from_raw(entry.uid()))
                .gid(Gid::from_raw(entry.gid()))
                .build();
            if sflag.contains(SFlag::S_IFDIR) {
                fs.insert(path, Directory::builder().metadata(metadata).build());
            } else if sflag.contains(SFlag::S_IFLNK) {
                let name_size = entry.file_size() as usize;
                // the symlink target starts at the header_start + HEADER_LEN +
                // path + NUL, padded to the next multiple of 4
                let link_start =
                    align_to_4_bytes(header_start_pos + HEADER_LEN + entry.name().len() + 1);
                let target = contents.slice(link_start..link_start + name_size);
                fs.insert(path, Symlink::new(target, Some(metadata)));
            } else if sflag.contains(SFlag::S_IFREG) {
                let file_size = entry.file_size() as usize;
                // the file starts at the header_start + HEADER_LEN + path +
                // NUL, padded to the next multiple of 4
                let file_start =
                    align_to_4_bytes(header_start_pos + HEADER_LEN + entry.name().len() + 1);
                let file_contents = contents.slice(file_start..file_start + file_size);
                fs.insert(
                    path,
                    File::builder()
                        .contents(file_contents)
                        .metadata(metadata)
                        .build(),
                );
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
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;
    use crate::cmp::assert_approx_eq;
    use crate::cmp::Fields;
    use crate::tests::demo_fs;
    use crate::BytesPath;

    #[rstest]
    #[case(0, 0)]
    #[case(1, 4)]
    #[case(2, 4)]
    #[case(3, 4)]
    #[case(4, 4)]
    #[case(5, 8)]
    #[case(6, 8)]
    fn align(#[case] pos: usize, #[case] expected: usize) {
        assert_eq!(expected, align_to_4_bytes(pos));
    }

    #[test]
    fn cpio() {
        let contents = Bytes::from_static(include_bytes!("../../testdata/testdata.cpio"));
        let fs = Filesystem::parse_cpio(&contents).expect("failed to parse cpio");
        let mut demo_fs = demo_fs();
        // cpio is missing the top-level directory
        demo_fs.unlink(&BytesPath::from("")).unwrap();
        // cpio does not support xattrs
        assert_approx_eq!(demo_fs, fs, Fields::all() - Fields::XATTR);
    }
}
