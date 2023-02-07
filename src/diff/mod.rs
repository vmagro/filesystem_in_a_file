use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

use console::style;
use console::Style;
use similar::Algorithm;
use similar::ChangeTag;
use similar::TextDiff;

use crate::cmp::ApproxEq;
use crate::cmp::Fields;
use crate::entry::Entry;
use crate::Filesystem;

mod diffable;
use diffable::Diffable;

#[derive(Debug)]
pub enum Diff<T> {
    /// Right side removed this object that existed in the left side
    Removed(T),
    /// Right side added this object that did not exist in the left side
    Added(T),
    /// Left and right side both contain this object but it is different somehow
    Changed { left: T, right: T },
}

impl<T> Display for Diff<T>
where
    T: Diffable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (left, right) = match self {
            Self::Removed(removed) => (removed.to_diffable_string(), Cow::Borrowed("")),
            Self::Added(added) => (Cow::Borrowed(""), added.to_diffable_string()),
            Self::Changed { left, right } => {
                (left.to_diffable_string(), right.to_diffable_string())
            }
        };
        let diff = TextDiff::configure()
            .timeout(Duration::from_millis(200))
            .algorithm(Algorithm::Patience)
            .diff_lines(&left, &right);

        writeln!(
            f,
            "{} ({}{}|{}{}):",
            style("Differences").bold(),
            style("-").red().dim(),
            style("left").red(),
            style("+").green().dim(),
            style("right").green(),
        )?;
        for op in diff.ops() {
            for change in diff.iter_inline_changes(&op) {
                let (marker, style) = match change.tag() {
                    ChangeTag::Delete => ('-', Style::new().red()),
                    ChangeTag::Insert => ('+', Style::new().green()),
                    ChangeTag::Equal => (' ', Style::new().dim()),
                };
                write!(f, "{}", style.apply_to(marker).dim().bold())?;
                for &(emphasized, value) in change.values() {
                    if emphasized {
                        write!(f, "{}", style.clone().underlined().bold().apply_to(value))?;
                    } else {
                        write!(f, "{}", style.apply_to(value))?;
                    }
                }
                if change.missing_newline() {
                    writeln!(f)?;
                }
            }
            // }
        }
        Ok(())
    }
}

pub struct FilesystemDiff<'b> {
    entry_diffs: BTreeMap<&'b Path, Diff<&'b Entry>>,
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
            writeln!(f, "Entry at {}:", path.display())?;
            writeln!(f, "{}", diff)?;
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
                        .uid(Uid::current())
                        .gid(Gid::current())
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
