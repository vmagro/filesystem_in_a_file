use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

use similar::udiff::unified_diff;
use similar::Algorithm;

use crate::cmp::ApproxEq;
use crate::cmp::Fields;
use crate::entry::Entry;
use crate::Filesystem;

mod diffable;
use diffable::DiffSection;
use diffable::Diffable;

#[derive(Debug)]
pub enum Diff<T, const N: usize>
where
    T: for<'a> Diffable<'a, N>,
{
    /// Right side removed this object that existed in the left side
    Removed(T),
    /// Right side added this object that did not exist in the left side
    Added(T),
    /// Left and right side both contain this object but it is different somehow
    Changed { left: T, right: T },
}

impl<T, const N: usize> Display for Diff<T, N>
where
    T: for<'a> Diffable<'a, N>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (left, right) = match self {
            Self::Removed(removed) => {
                let removed = removed.to_diffable_sections();
                let empty = removed.clone().map(|s| DiffSection {
                    title: s.title,
                    contents: Cow::Borrowed(""),
                });
                (removed, empty)
            }
            Self::Added(added) => {
                let added = added.to_diffable_sections();
                let empty = added.clone().map(|s| DiffSection {
                    title: s.title,
                    contents: Cow::Borrowed(""),
                });
                (empty, added)
            }
            Self::Changed { left, right } => {
                (left.to_diffable_sections(), right.to_diffable_sections())
            }
        };

        for (left, right) in left.iter().zip(right.iter()) {
            writeln!(f, "{}", left.title)?;
            debug_assert_eq!(left.title, right.title);
            let diff = unified_diff(
                Algorithm::Patience,
                &left.contents,
                &right.contents,
                3,
                None,
            );
            f.write_str(&diff)?;
        }
        Ok(())
    }
}

pub struct FilesystemDiff<'b> {
    entry_diffs: BTreeMap<&'b Path, Diff<&'b Entry, 3>>,
}

impl<'b> FilesystemDiff<'b> {
    pub fn diff(left: &'b Filesystem, right: &'b Filesystem, fields: Fields) -> Self {
        let mut diffs = BTreeMap::new();
        for (path, left_entry) in left.iter() {
            match right.get(path) {
                Ok(right_entry) => {
                    if !left_entry.approx_eq(right_entry, fields) {
                        diffs.insert(
                            path,
                            Diff::Changed {
                                left: left_entry,
                                right: right_entry,
                            },
                        );
                    }
                }
                Err(_) => {
                    diffs.insert(path, Diff::Removed(left_entry));
                }
            };
        }
        for (path, right_entry) in right.iter() {
            if left.get(path).is_err() {
                diffs.insert(path, Diff::Added(right_entry));
            }
        }
        Self { entry_diffs: diffs }
    }
}

impl<'b> Display for FilesystemDiff<'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (path, diff) in &self.entry_diffs {
            writeln!(f, "Entry at '{}':", path.display())?;
            writeln!(f, "{diff}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nix::sys::stat::Mode;
    use nix::unistd::Gid;
    use nix::unistd::Uid;
    use similar_asserts::assert_eq;

    use super::*;
    use crate::entry::Metadata;
    use crate::entry::Symlink;
    use crate::tests::demo_fs;
    use crate::File;

    #[test]
    fn whole_fs_diff_is_useful() {
        let left = demo_fs();
        let mut right = demo_fs();
        right.insert(
            "testdata/dir/lorem.txt",
            File::builder()
                .contents("Lorem ipsum consectetur adipiscing elit,\nsed do eiusmod")
                .metadata(
                    Metadata::builder()
                        .mode(Mode::from_bits_truncate(0o444))
                        .uid(Uid::from_raw(1000))
                        .gid(Gid::from_raw(1000))
                        .build(),
                )
                .build(),
        );
        right.insert("testdata/dir/symlink", Symlink::new("./lorem.txt", None));
        let diff = FilesystemDiff::diff(&left, &right, Fields::all());
        assert_eq!(
            console::strip_ansi_codes(&diff.to_string()),
            include_str!("../../testdata/whole_fs_diff.txt"),
        );
    }
}
