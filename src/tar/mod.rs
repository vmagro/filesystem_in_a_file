use std::borrow::Cow;
use std::io::Cursor;

use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use tar::Archive;
use tar::EntryType;

use crate::File;
use crate::Filesystem;

impl<'f> Filesystem<'f, 'f> {
    /// Create an in-memory view of a Filesystem from the contents of a tarball.
    pub fn from_tar(contents: &'f [u8]) -> std::io::Result<Self> {
        let mut fs = Self::new();
        for entry in Archive::new(Cursor::new(contents)).entries_with_seek()? {
            let entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let path = Cow::Owned(entry.path()?.to_path_buf());
            if entry.header().entry_type() == EntryType::Regular {
                fs.files.insert(
                    path,
                    File::builder()
                        .contents(&contents[file_offset..file_offset + entry.size() as usize])
                        .mode(Mode::from_bits_truncate(entry.header().mode()?))
                        .uid(Uid::from_raw(entry.header().uid()? as u32))
                        .gid(Gid::from_raw(entry.header().gid()? as u32))
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
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;

    #[test]
    fn tar() {
        let fs = Filesystem::from_tar(include_bytes!("testdata/testdata.tar"))
            .expect("failed to parse tar");
        assert_eq!(
            fs,
            Filesystem {
                files: BTreeMap::from([
                    (
                        Path::new("./lorem.txt").into(),
                        File::builder()
                            .contents(b"Lorem ipsum\n")
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .build()
                            .into()
                    ),
                    (
                        Path::new("./dir/lorem.txt").into(),
                        File::builder()
                            .contents(b"Lorem ipsum dolor sit amet\n")
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .build()
                            .into()
                    ),
                ]),
            }
        );
    }
}
