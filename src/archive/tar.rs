use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::Cursor;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use tar::Archive;
use tar::EntryType;

use crate::entry::Directory;
use crate::entry::Metadata;
use crate::entry::Symlink;
use crate::File;
use crate::Filesystem;

// See https://www.gnu.org/software/tar/manual/html_node/Standard.html for some
// of the offsets used here to get borrows to the underlying slice

impl<'f> Filesystem<'f> {
    /// Load an uncompressed tarball.
    pub fn parse_tar(contents: &'f [u8]) -> std::io::Result<Self> {
        let mut fs = Filesystem::new();
        for entry in Archive::new(Cursor::new(&contents)).entries_with_seek()? {
            let entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let path = Path::new(OsStr::from_bytes(
                &contents[entry.raw_header_position() as usize
                    ..entry.raw_header_position() as usize + entry.path_bytes().len()],
            ));
            match entry.header().entry_type() {
                EntryType::Directory => {
                    let path = path.as_os_str().as_bytes();
                    let path = &path[..path.len() - 1];
                    fs.entries.insert(
                        Path::new(OsStr::from_bytes(path)),
                        Directory::builder()
                            .metadata(Metadata::try_from_entry(entry)?)
                            .build()
                            .into(),
                    );
                }
                EntryType::Regular => {
                    fs.entries.insert(
                        path,
                        File::builder()
                            .contents(&contents[file_offset..file_offset + entry.size() as usize])
                            .metadata(Metadata::try_from_entry(entry)?)
                            .build()
                            .into(),
                    );
                }
                EntryType::Symlink => {
                    let link_target = Path::new(OsStr::from_bytes(
                        &contents[entry.raw_header_position() as usize + 157
                            ..entry.raw_header_position() as usize
                                + 157
                                + entry
                                    .link_name_bytes()
                                    .expect("symlink must have link name")
                                    .len()],
                    ));
                    fs.entries.insert(
                        path.into(),
                        Symlink::new(link_target, Some(Metadata::try_from_entry(entry)?)).into(),
                    );
                }
                ty => {
                    todo!("unhandled entry type {ty:?}");
                }
            };
        }
        Ok(fs)
    }
}

impl<'f> Metadata<'f> {
    fn try_from_entry<R: Read>(mut entry: tar::Entry<R>) -> std::io::Result<Self> {
        let mut xattrs = BTreeMap::new();
        if let Ok(Some(pax_extensions)) = entry.pax_extensions() {
            for ext in pax_extensions.into_iter().filter_map(Result::ok) {
                // if ext.key_bytes().starts_with(b"SCHILY.xattr.") {
                //     xattrs.insert(
                //         OsString::from_vec(ext.key_bytes()["SCHILY.xattr.".len()..].to_vec()),
                //         ext.value_bytes().to_vec(),
                //     );
                // }
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

    use memmap::MmapOptions;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn tar() {
        let file = std::fs::File::open(Path::new(env!("OUT_DIR")).join("testdata.tar"))
            .expect("failed to open testdata.tar");
        let contents = unsafe { MmapOptions::new().map(&file).unwrap() };
        let fs = Filesystem::parse_tar(&contents).expect("failed to parse tar");
        let mut demo_fs = demo_fs();
        // tar is missing the top-level directory
        demo_fs.entries.remove(Path::new(""));
        assert_eq!(demo_fs, fs);
    }
}
