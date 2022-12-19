use std::borrow::Cow;
use std::path::Path;

use sendstream_parser::Command;
use sendstream_parser::Error;
use sendstream_parser::Sendstream;

use crate::entry::Directory;
use crate::Filesystem;

pub struct Sendstreams<'s> {
    sendstreams: Vec<Sendstream<'s>>,
}

impl<'s> Sendstreams<'s> {
    pub fn new(sendstreams: Vec<Sendstream<'s>>) -> Self {
        Self { sendstreams }
    }
}

macro_rules! subvol_path {
    ($subvol:ident, $path:expr) => {
        &Cow::Owned($subvol.path().join($path))
    };
}

impl<'s> Sendstreams<'s> {
    pub fn filesystem(&self) -> Result<Filesystem<'_, '_>, Error> {
        let mut fs = Filesystem::new();
        for sendstream in &self.sendstreams {
            let mut cmd_iter = sendstream.commands().iter();
            let subvol = match cmd_iter.next().expect("must have at least one command") {
                Command::Subvol(s) => s,
                _ => panic!("first command is always the subvolume"),
            };
            fs.entries
                .insert(subvol.path().clone(), Directory::default().into());
            for command in cmd_iter {
                eprintln!("{command:?}");
                match command {
                    Command::Chown(c) => {
                        fs.entries
                            .get_mut(subvol_path!(subvol, c.path()))
                            .expect("must exist")
                            .chown(c.uid(), c.gid());
                    }
                    Command::Chmod(c) => fs
                        .entries
                        .get_mut(subvol_path!(subvol, c.path()))
                        .expect("must exist")
                        .chmod(c.mode()),
                    // Command::Mkfile(m) => {

                    // }
                    _ => {}
                }
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
    fn sendstream() {
        let file = std::fs::File::open(Path::new(env!("OUT_DIR")).join("testdata.sendstream"))
            .expect("failed to open testdata.sendstream");
        let contents =
            unsafe { memmap::MmapOptions::new().map(&file) }.expect("failed to mmap sendstream");
        let sendstreams = Sendstream::parse_all(&contents).expect("failed to parse sendstream");
        let sendstreams = Sendstreams::new(sendstreams);
        let fs = sendstreams.filesystem().expect("failed to create fs");
        let mut demo_fs = demo_fs();
        assert_eq!(fs, demo_fs);
    }
}
