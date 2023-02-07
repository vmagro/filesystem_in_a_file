use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Write;
use std::path::Path;

use similar::udiff::unified_diff;
use similar::Algorithm;

use crate::cmp::ApproxEq;
use crate::cmp::Fields;
use crate::entry::Entry;
use crate::Filesystem;

mod diffable;
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
            Self::Removed(removed) => (
                removed.to_diffable_sections(),
                std::array::from_fn(|_| Cow::Borrowed("")),
            ),
            Self::Added(added) => (
                std::array::from_fn(|_| Cow::Borrowed("")),
                added.to_diffable_sections(),
            ),
            Self::Changed { left, right } => {
                (left.to_diffable_sections(), right.to_diffable_sections())
            }
        };

        for (title, (left, right)) in T::SECTIONS.iter().zip(left.iter().zip(right.iter())) {
            if left == right {
                continue;
            }
            writeln!(f, "{title}")?;
            if left.matches('\n').count() <= 1 && right.matches('\n').count() <= 1 {
                writeln!(f, "-{left}")?;
                writeln!(f, "+{right}")?;
            } else {
                let diff = unified_diff(Algorithm::Patience, &left, &right, 3, None);
                f.write_str(&diff)?;
            }
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
        let mut iter = self.entry_diffs.iter().peekable();
        while let Some((path, diff)) = iter.next() {
            match diff {
                Diff::Added(_) => {
                    writeln!(f, "--- /dev/null")?;
                    writeln!(f, "+++ right/{}", path.display())?;
                }
                Diff::Removed(_) => {
                    writeln!(f, "--- left/{}", path.display())?;
                    writeln!(f, "+++ /dev/null")?;
                }
                Diff::Changed { .. } => {
                    writeln!(f, "---  left/{}", path.display())?;
                    writeln!(f, "+++ right/{}", path.display())?;
                }
            }
            writeln!(f, "{}", diff.to_string().trim_end_matches('\n'))?;
            if iter.peek().is_some() {
                f.write_char('\n')?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nix::sys::stat::Mode;
    use similar_asserts::assert_eq;

    use super::*;
    use crate::entry::Metadata;
    use crate::entry::Symlink;
    use crate::tests::demo_fs;
    use crate::File;
    use crate::Gid;
    use crate::Uid;

    #[test]
    fn whole_fs_diff_is_useful() {
        let left = demo_fs();
        let mut right = demo_fs();
        right.insert(
            "testdata/dir/lorem.txt",
            File::builder()
                .contents("Lorem ipsum consectetur adipiscing elit,\nsed do eiusmod\n")
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
        assert_eq!(diff.to_string(), include_str!("testdata/whole_fs_diff.txt"),);
    }

    #[test]
    fn simple_image_feature_diff() {
        let mut left = demo_fs();
        left.insert(
            "etc/passwd",
            File::builder()
                .contents(include_str!("testdata/passwd.before"))
                .build(),
        );
        let mut right = left.clone();
        right.insert(
            "etc/passwd",
            File::builder()
                .contents(include_str!("testdata/passwd.after"))
                .build(),
        );
        let diff = FilesystemDiff::diff(&left, &right, Fields::all());
        assert_eq!(diff.to_string(), include_str!("testdata/passwd_diff.txt"),);
    }
}
