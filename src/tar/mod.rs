use std::borrow::Cow;
use std::ffi::OsString;
use std::io::Cursor;
use std::os::unix::ffi::OsStringExt;

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
            let mut entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let path = Cow::Owned(entry.path()?.to_path_buf());
            if entry.header().entry_type() == EntryType::Regular {
                let mut builder = File::builder();
                builder
                    .contents(&contents[file_offset..file_offset + entry.size() as usize])
                    .mode(Mode::from_bits_truncate(entry.header().mode()?))
                    .uid(Uid::from_raw(entry.header().uid()? as u32))
                    .gid(Gid::from_raw(entry.header().gid()? as u32));

                if let Ok(Some(pax_extensions)) = entry.pax_extensions() {
                    for ext in pax_extensions.into_iter().filter_map(Result::ok) {
                        if ext.key_bytes().starts_with(b"SCHILY.xattr.") {
                            builder.xattr(
                                OsString::from_vec(
                                    ext.key_bytes()["SCHILY.xattr.".len()..].to_vec(),
                                ),
                                ext.value_bytes().to_vec(),
                            );
                        }
                    }
                }
                fs.entries.insert(path, builder.build().into());
            }
        }
        Ok(fs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::path::Path;

    use super::*;

    #[test]
    fn tar() {
        let file = std::fs::File::open(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tar/testdata/testdata.tar"),
        )
        .expect("failed to open testdata.tar");
        let testdata_tar = unsafe {
            memmap::MmapOptions::new()
                .map(&file)
                .expect("failed to mmap testdata.tar")
        };
        let fs = Filesystem::from_tar(&testdata_tar).expect("failed to parse tar");
        assert_eq!(
            fs,
            Filesystem {
                entries: BTreeMap::from([
                    (
                        Path::new("./lorem.txt").into(),
                        File::builder()
                            .contents(b"Lorem ipsum\n")
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .xattr(OsStr::new("user.demo"), &b"lorem ipsum"[..])
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
