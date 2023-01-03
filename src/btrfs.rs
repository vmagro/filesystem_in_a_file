use std::collections::BTreeMap;
use std::io::Seek;
use std::io::SeekFrom;

use bytes::Bytes;
use sendstream_parser::Command;
use sendstream_parser::Sendstream;
use uuid::Uuid;

use crate::entry::Directory;
use crate::entry::Entry;
use crate::entry::Special;
use crate::entry::Symlink;
use crate::file::File;
use crate::Filesystem;

#[derive(thiserror::Error, Debug)]
pub enum Error<'c> {
    #[error("invariant violated: {0}")]
    InvariantViolated(&'static str),
    #[error("parent subvol not yet received: {0}")]
    MissingParent(Uuid),
    #[error(transparent)]
    Parse(sendstream_parser::Error<'c>),
    #[error("failed to apply {command:?}: {error:?}")]
    Apply {
        command: Command<'c>,
        error: crate::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subvol {
    parent_uuid: Option<Uuid>,
    fs: Filesystem,
}

impl Subvol {
    fn new() -> Self {
        Subvol {
            parent_uuid: None,
            fs: Filesystem::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subvols(BTreeMap<Uuid, Subvol>);

impl Subvols {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    #[remain::check]
    fn apply_cmd(subvol: &mut Subvol, cmd: &Command<'_>) -> crate::Result<()> {
        #[remain::sorted]
        match cmd {
            Command::Chmod(c) => subvol.fs.chmod(c.path(), c.mode().mode()),
            Command::Chown(c) => subvol.fs.chown(c.path(), c.uid(), c.gid()),
            Command::Clone(c) => {
                let src = subvol.fs.get(c.src_path())?;
                let extents = match src {
                    Entry::File(f) => {
                        let start = c.src_offset().as_u64();
                        Ok(f.clone_range(start..start + c.len().as_u64()))
                    }
                    _ => Err(crate::Error::WrongEntryType),
                }?;
                let dst = subvol.fs.get_mut(c.dst_path())?;
                match dst {
                    Entry::File(f) => {
                        let mut wr = f.writer();
                        wr.seek(SeekFrom::Start(c.dst_offset().as_u64()))
                            .expect("infallible");
                        for ex in extents {
                            wr.write(ex);
                        }
                        Ok(())
                    }
                    _ => Err(crate::Error::WrongEntryType),
                }
            }
            Command::Link(l) => subvol.fs.link(l.target().as_path(), l.link_name()),
            Command::Mkdir(m) => {
                subvol.fs.insert(m.path().as_path(), Directory::default());
                Ok(())
            }
            Command::Mkfifo(ref m) => {
                subvol.fs.insert(
                    m.path().as_path(),
                    Special::new(m.mode().file_type(), Default::default()),
                );
                Ok(())
            }
            Command::Mkfile(m) => {
                subvol.fs.insert(m.path().as_path(), File::default());
                Ok(())
            }
            Command::Mksock(ref m) => {
                subvol.fs.insert(
                    m.path().as_path(),
                    Special::new(m.mode().file_type(), Default::default()),
                );
                Ok(())
            }
            Command::Rename(r) => subvol.fs.rename(r.from(), r.to()),
            Command::SetXattr(s) => {
                subvol.fs.get_mut(s.path())?.metadata_mut().xattrs.insert(
                    Bytes::copy_from_slice(s.name()),
                    Bytes::copy_from_slice(s.data()),
                );
                Ok(())
            }
            Command::Symlink(s) => {
                subvol
                    .fs
                    .insert(s.link_name(), Symlink::new(s.target().as_path(), None));
                Ok(())
            }
            Command::Utimes(u) => subvol
                .fs
                .set_times(u.path(), *u.ctime(), *u.atime(), *u.mtime()),
            Command::Write(w) => match subvol.fs.get_mut(w.path())? {
                Entry::File(f) => {
                    let mut wr = f.writer();
                    wr.seek(SeekFrom::Start(w.offset().as_u64()))
                        .expect("infallible");
                    wr.write(Bytes::copy_from_slice(w.data().as_slice()));
                    Ok(())
                }
                _ => Err(crate::Error::WrongEntryType),
            },
            _ => {
                todo!("unimplemented command: {:?}", cmd);
            }
        }
    }

    /// Parse subvolumes from an uncompressed sendstream
    pub fn receive<'f>(&mut self, sendstream: Sendstream<'f>) -> Result<(), Error<'f>> {
        let mut cmd_iter = sendstream.into_commands().into_iter();
        let (subvol_uuid, mut subvol) = #[remain::sorted]
        match cmd_iter
            .next()
            .expect("must have at least one command")
        {
            Command::Snapshot(s) => {
                let mut subvol = self
                    .0
                    .get(&s.clone_uuid())
                    .ok_or(Error::MissingParent(s.clone_uuid()))?
                    .clone();
                subvol.parent_uuid = Some(s.clone_uuid());
                (s.uuid(), subvol)
            }
            Command::Subvol(s) => {
                let mut subvol = Subvol::new();
                subvol.fs.insert("", Directory::default());
                (s.uuid(), subvol)
            }
            _ => return Err(Error::InvariantViolated("first command was not subvol start").into()),
        };
        for cmd in cmd_iter {
            Self::apply_cmd(&mut subvol, &cmd).map_err(|error| Error::Apply {
                command: cmd,
                error,
            })?;
        }
        self.0.insert(subvol_uuid, subvol);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bytes::Bytes;
    use pretty_assertions::assert_eq;
    use uuid::uuid;

    use super::*;
    use crate::tests::demo_fs;

    #[test]
    fn sendstream() {
        let contents = Bytes::from(
            std::fs::read(Path::new(env!("OUT_DIR")).join("testdata.sendstream"))
                .expect("failed to read testdata.sendstream"),
        );
        let sendstreams = Sendstream::parse_all(&contents).expect("failed to parse sendstream");
        let mut subvols = Subvols::new();
        for sendstream in sendstreams {
            subvols
                .receive(sendstream)
                .expect("failed to receive sendstream");
        }
        assert_eq!(
            BTreeMap::from([
                (
                    uuid!("0fbf2b5f-ff82-a748-8b41-e35aec190b49"),
                    Subvol {
                        parent_uuid: None,
                        fs: demo_fs(),
                    }
                ),
                (
                    uuid!("ed2c87d3-12e3-c549-a699-635de66d6f35"),
                    Subvol {
                        parent_uuid: Some(uuid!("0fbf2b5f-ff82-a748-8b41-e35aec190b49")),
                        fs: demo_fs(),
                    }
                )
            ]),
            subvols.0
        );
    }
}
