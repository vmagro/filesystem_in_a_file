use std::collections::BTreeMap;
use std::io::Cursor;
use std::io::Read;

use bytes::Bytes;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use tar::Archive;
use tar::EntryType;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::entry::Symlink;
use crate::BytesExt;
use crate::BytesPath;
use crate::File;
use crate::Filesystem;

// See https://www.gnu.org/software/tar/manual/html_node/Standard.html for some
// of the offsets used here to get borrows to the underlying slice

impl Filesystem {
    /// Load an uncompressed tarball.
    pub fn parse_tar(contents: &Bytes) -> std::io::Result<Self> {
        let mut fs = Filesystem::new();
        for entry in Archive::new(Cursor::new(&contents)).entries_with_seek()? {
            let mut entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let mut path: BytesPath = contents.subslice_or_copy(&entry.path_bytes()).into();
            let metadata = Metadata::try_from_entry(contents, &mut entry)?;
            match entry.header().entry_type() {
                EntryType::Directory => {
                    // remove trailing / for consistency
                    let new_len = path.len() - 1;
                    path.bytes_mut().truncate(new_len);
                    fs.insert(path, Directory::builder().metadata(metadata).build());
                }
                EntryType::Regular => {
                    fs.insert(
                        path,
                        File::builder()
                            .contents(
                                contents.slice(file_offset..file_offset + entry.size() as usize),
                            )
                            .metadata(metadata)
                            .build(),
                    );
                }
                EntryType::Symlink => {
                    let link_target = contents.subslice_or_copy(
                        &entry
                            .link_name_bytes()
                            .expect("symlink must have link target"),
                    );
                    fs.insert(path, Symlink::new(link_target, Some(metadata)));
                }
                ty => {
                    todo!("unhandled entry type {ty:?}");
                }
            };
        }
        Ok(fs)
    }
}

impl Metadata {
    fn try_from_entry<R: Read>(
        contents: &Bytes,
        entry: &mut tar::Entry<R>,
    ) -> std::io::Result<Self> {
        let mut xattrs = BTreeMap::new();
        if let Ok(Some(pax_extensions)) = entry.pax_extensions() {
            for ext in pax_extensions.into_iter().filter_map(Result::ok) {
                if ext.key_bytes().starts_with(b"SCHILY.xattr.") {
                    xattrs.insert(
                        contents.subslice_or_copy(&ext.key_bytes()["SCHILY.xattr.".len()..]),
                        contents.subslice_or_copy(ext.value_bytes()),
                    );
                }
            }
        }
        Ok(Metadata::builder()
            .mode(Mode::from_bits_truncate(entry.header().mode()?))
            .uid(Uid::from_raw(entry.header().uid()? as u32))
            .gid(Gid::from_raw(entry.header().gid()? as u32))
            .xattrs(xattrs)
            .build())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tests::demo_fs;
    use crate::BytesPath;

    #[test]
    fn tar() {
        let contents = Bytes::from(
            std::fs::read(Path::new(env!("OUT_DIR")).join("testdata.tar"))
                .expect("failed to read testdata.tar"),
        );
        let fs = Filesystem::parse_tar(&contents).expect("failed to parse tar");
        let mut demo_fs = demo_fs();
        // tar is missing the top-level directory
        demo_fs.unlink(&BytesPath::from(""));
        assert_eq!(demo_fs, fs);
    }
}
