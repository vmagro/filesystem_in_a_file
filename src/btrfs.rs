use std::collections::BTreeMap;
use std::io::Seek;
use std::io::SeekFrom;
use std::ops::Deref;

use anyhow::anyhow;
use bytes::Bytes;
use sendstream_parser::Command;
use sendstream_parser::Sendstream;
use uuid::Uuid;

use crate::entry::Directory;
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
    fn apply_cmd(&mut self, subvol: &mut Subvol, cmd: &Command<'_>) -> crate::Result<()> {
        #[remain::sorted]
        match cmd {
            Command::Chmod(c) => subvol.fs.chmod(c.path(), c.mode().mode()),
            Command::Chown(c) => subvol.fs.chown(c.path(), c.uid(), c.gid()),
            Command::Clone(c) => {
                let src = subvol.fs.get_file(c.src_path())?;
                let start = c.src_offset().as_u64();
                let extents = src.clone_range(start..start + c.len().as_u64());
                let dst = subvol.fs.get_file_mut(c.dst_path())?;
                let mut wr = dst.writer();
                wr.seek(SeekFrom::Start(c.dst_offset().as_u64()))
                    .expect("infallible");
                for ex in extents {
                    wr.write(ex);
                }
                Ok(())
            }
            Command::End => Ok(()),
            Command::Link(l) => subvol.fs.link(l.target().as_path(), l.link_name()),
            Command::Mkdir(m) => {
                subvol.fs.insert(m.path().as_path(), Directory::default());
                Ok(())
            }
            Command::Mkfifo(ref m) => {
                subvol.fs.insert(
                    m.path().as_path(),
                    Special::new(m.mode().file_type(), *m.rdev(), Default::default()),
                );
                Ok(())
            }
            Command::Mkfile(m) => {
                subvol.fs.insert(m.path().as_path(), File::default());
                Ok(())
            }
            Command::Mknod(m) => {
                subvol.fs.insert(
                    m.path().as_path(),
                    Special::new(m.mode().file_type(), *m.rdev(), Default::default()),
                );
                Ok(())
            }
            Command::Mksock(m) => {
                subvol.fs.insert(
                    m.path().as_path(),
                    Special::new(m.mode().file_type(), *m.rdev(), Default::default()),
                );
                Ok(())
            }
            Command::RemoveXattr(r) => {
                subvol
                    .fs
                    .get_mut(r.path())?
                    .metadata_mut()
                    .xattrs
                    .retain(|k, _| k != r.name().deref());
                Ok(())
            }
            Command::Rename(r) => subvol.fs.rename(r.from(), r.to()),
            Command::Rmdir(r) => subvol.fs.rmdir(r.path()),
            Command::SetXattr(s) => {
                subvol.fs.get_mut(s.path())?.metadata_mut().xattrs.insert(
                    Bytes::copy_from_slice(s.name()),
                    Bytes::copy_from_slice(s.data()),
                );
                Ok(())
            }
            Command::Snapshot(s) => {
                todo!()
            }
            Command::Subvol(s) => {
                todo!()
            }
            Command::Symlink(s) => {
                subvol
                    .fs
                    .insert(s.link_name(), Symlink::new(s.target().as_path(), None));
                Ok(())
            }
            Command::Truncate(t) => subvol.fs.truncate(t.path(), t.size()),
            Command::Unlink(u) => subvol.fs.unlink(u.path()),
            Command::UpdateExtent(_) => {
                Err(anyhow!("UpdateExtent command is not supported").into())
            }
            Command::Utimes(u) => subvol
                .fs
                .set_times(u.path(), *u.ctime(), *u.atime(), *u.mtime()),
            Command::Write(w) => {
                let f = subvol.fs.get_file_mut(w.path())?;
                let mut wr = f.writer();
                wr.seek(SeekFrom::Start(w.offset().as_u64()))
                    .expect("infallible");
                wr.write(Bytes::copy_from_slice(w.data().as_slice()));
                Ok(())
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
            self.apply_cmd(&mut subvol, &cmd)
                .map_err(|error| Error::Apply {
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
    use std::collections::HashSet;
    use std::path::Path;

    use bytes::Bytes;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cmp::assert_approx_eq;
    use crate::cmp::Fields;
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
        // drop the uuid which will change on every build and re-order so that
        // the parent is always first
        let uuids: HashSet<Uuid> = subvols.0.keys().map(|u| *u).collect();
        let mut subvols: Vec<_> = subvols.0.into_values().collect();
        assert_eq!(2, subvols.len());
        subvols.sort_by_key(|s| s.parent_uuid);
        let parent_uuid = subvols[1].parent_uuid.unwrap();
        assert!(uuids.contains(&parent_uuid));
        assert_approx_eq!(demo_fs(), &subvols[0].fs, Fields::all() - Fields::TIME);
        assert_eq!(
            vec![
                Subvol {
                    parent_uuid: None,
                    fs: demo_fs(),
                },
                Subvol {
                    parent_uuid: Some(parent_uuid),
                    fs: demo_fs(),
                }
            ],
            subvols
        );
    }
}
