use std::collections::BTreeMap;

use sendstream_parser::Command;
use sendstream_parser::Sendstream;
use uuid::Uuid;

use crate::entry::Directory;
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

    fn apply_cmd(subvol: &mut Subvol, cmd: &Command<'_>) -> crate::Result<()> {
        match cmd {
            Command::Chmod(c) => subvol.fs.chmod(c.path(), c.mode().mode()),
            Command::Mkdir(m) => {
                subvol.fs.insert(m.path().as_path(), Directory::default());
                Ok(())
            }
            Command::Mkfile(m) => {
                subvol.fs.insert(m.path().as_path(), File::default());
                Ok(())
            }
            Command::Rename(r) => subvol.fs.rename(r.from(), r.to()),
            _ => {
                eprintln!("unimplemented command: {:?}", cmd);
                Ok(())
            }
        }
    }

    /// Parse subvolumes from an uncompressed sendstream
    pub fn receive<'f>(&mut self, sendstream: Sendstream<'f>) -> Result<(), Error<'f>> {
        let mut cmd_iter = sendstream.into_commands().into_iter();
        let (subvol_uuid, mut subvol) = match cmd_iter
            .next()
            .expect("must have at least one command")
        {
            Command::Subvol(s) => {
                let mut subvol = Subvol::new();
                subvol.fs.insert("", Directory::default());
                (s.uuid(), subvol)
            }
            Command::Snapshot(s) => {
                let mut subvol = self
                    .0
                    .get(&s.clone_uuid())
                    .ok_or(Error::MissingParent(s.clone_uuid()))?
                    .clone();
                subvol.parent_uuid = Some(s.clone_uuid());
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
