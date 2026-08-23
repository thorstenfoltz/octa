use crate::data::{CellValue, DataTable};
use std::collections::BTreeMap;

/// One DataTable per distinct value of `col`. Null groups under "__null__".
/// Where the partition files land.
///
/// Two independent questions - is there a folder per value, and is the file
/// named after the value or numbered - would be a two-by-two matrix to
/// explain. Presenting the four useful combinations as one list instead is
/// what makes the dialog readable: the user picks a shape, not two switches.
///
/// `Flat` is the original behaviour and stays the default. All four write the
/// same rows, and all four can be reopened as one table (the partition column
/// is written into every file), so the choice is only about the names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartitionLayout {
    /// `new_york.csv` - one file per value, named after it.
    #[default]
    Flat,
    /// `New York/part-0001.csv` - a folder per value, numbered file inside.
    Folder,
    /// `city=New York/data.csv` - the lakehouse convention.
    Hive,
    /// `city=New York/part-0001.csv` - the lakehouse convention, with the
    /// part-file naming Spark and friends actually write.
    HiveParts,
}

impl PartitionLayout {
    /// Every layout, in the order the dialog lists them.
    pub const ALL: &'static [PartitionLayout] =
        &[Self::Flat, Self::Folder, Self::Hive, Self::HiveParts];

    /// The CLI / MCP word for this layout.
    pub fn id(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Folder => "folder",
            Self::Hive => "hive",
            Self::HiveParts => "hive-parts",
        }
    }

    /// i18n key for the radio label.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Flat => "partition.layout_flat",
            Self::Folder => "partition.layout_folder",
            Self::Hive => "partition.layout_hive",
            Self::HiveParts => "partition.layout_hive_parts",
        }
    }

    /// i18n key for the tooltip.
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Flat => "partition.layout_flat_hint",
            Self::Folder => "partition.layout_folder_hint",
            Self::Hive => "partition.layout_hive_hint",
            Self::HiveParts => "partition.layout_hive_parts_hint",
        }
    }

    /// Parse the CLI / MCP word. `None` for anything else, so the caller can
    /// name the valid words in its error. `hive_parts` is accepted alongside
    /// `hive-parts` because both spellings are natural on a command line.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_ascii_lowercase().replace('_', "-");
        Self::ALL.iter().copied().find(|l| l.id() == t)
    }
}

/// Replace only what cannot appear in a path component.
///
/// Deliberately not `sanitize_sql_name`, which the flat layout uses for file
/// stems: a value that lands in a directory name has to survive being read
/// back out of it, so dots, dashes, spaces and case are kept as they are.
fn path_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// `<col>=<value>` for one Hive partition directory.
pub fn hive_dir_name(col: &str, value: &str) -> String {
    format!("{}={}", path_safe(col), path_safe(value))
}

/// The path one partition is written to, relative to the output folder.
///
/// **The single source of truth for partition naming.** The writer joins this
/// onto the output directory and the dialog shows it as a preview, so what is
/// on screen before you press Apply is literally what lands on disk. `index`
/// is the 1-based position of this partition among the groups, used only by
/// the numbered layouts.
pub fn partition_path(
    layout: PartitionLayout,
    col_name: &str,
    value: &str,
    ext: &str,
    index: usize,
) -> String {
    match layout {
        PartitionLayout::Flat => {
            format!("{}.{ext}", crate::sql::sanitize_sql_name(value))
        }
        PartitionLayout::Folder => {
            format!("{}/part-{index:04}.{ext}", path_safe(value))
        }
        PartitionLayout::Hive => {
            format!("{}/data.{ext}", hive_dir_name(col_name, value))
        }
        PartitionLayout::HiveParts => {
            format!("{}/part-{index:04}.{ext}", hive_dir_name(col_name, value))
        }
    }
}

/// Disambiguate a `Flat` name that two different values sanitise down to.
///
/// Only `Flat` needs this: it is the one layout whose naming is lossy, since
/// `sanitize_sql_name` folds case and punctuation, so `New York`, `new-york`
/// and `NEW_YORK` all arrive as `new_york`. The folder layouts keep the value
/// path-safe but otherwise intact, so their names are already distinct.
pub fn dedupe_flat_name(
    layout: PartitionLayout,
    rel: String,
    ext: &str,
    seen: &mut std::collections::HashMap<String, usize>,
) -> String {
    if layout != PartitionLayout::Flat {
        return rel;
    }
    let stem = rel.trim_end_matches(&format!(".{ext}")).to_string();
    let n = seen.entry(stem.clone()).or_insert(0);
    *n += 1;
    if *n == 1 {
        rel
    } else {
        format!("{stem}_{n}.{ext}")
    }
}

/// Row order within a group is preserved; groups are returned value-sorted.
pub fn partition_table(table: &DataTable, col: usize) -> Vec<(String, DataTable)> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for row in 0..table.row_count() {
        let key = match table.get(row, col) {
            Some(CellValue::Null) | None => "__null__".to_string(),
            Some(v) => v.to_string(),
        };
        groups.entry(key).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|(value, idxs)| {
            let mut out = DataTable::empty();
            out.columns = table.columns.clone();
            out.rows = idxs
                .iter()
                .map(|&r| {
                    (0..table.col_count())
                        .map(|c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                        .collect()
                })
                .collect();
            out.structural_changes = true;
            (value, out)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    #[test]
    fn splits_rows_by_value() {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "region".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "amt".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::String("US".into()), CellValue::Int(1)],
            vec![CellValue::String("EU".into()), CellValue::Int(2)],
            vec![CellValue::String("US".into()), CellValue::Int(3)],
        ];
        let parts = partition_table(&t, 0);
        let mut labels: Vec<_> = parts
            .iter()
            .map(|(v, tbl)| (v.clone(), tbl.row_count()))
            .collect();
        labels.sort();
        assert_eq!(labels, vec![("EU".to_string(), 1), ("US".to_string(), 2)]);
        assert_eq!(parts[0].1.columns.len(), 2);
    }

    #[test]
    fn null_groups_under_null_key() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vec![
            vec![CellValue::Null],
            vec![CellValue::String("A".into())],
            vec![CellValue::Null],
        ];
        let parts = partition_table(&t, 0);
        let null_group = parts.iter().find(|(k, _)| k == "__null__");
        assert!(null_group.is_some());
        assert_eq!(null_group.unwrap().1.row_count(), 2);
    }

    #[test]
    fn preserves_row_order_within_group() {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "g".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::String("X".into()), CellValue::Int(10)],
            vec![CellValue::String("X".into()), CellValue::Int(20)],
            vec![CellValue::String("X".into()), CellValue::Int(30)],
        ];
        let parts = partition_table(&t, 0);
        assert_eq!(parts.len(), 1);
        let rows = &parts[0].1;
        assert_eq!(rows.get(0, 1), Some(&CellValue::Int(10)));
        assert_eq!(rows.get(1, 1), Some(&CellValue::Int(20)));
        assert_eq!(rows.get(2, 1), Some(&CellValue::Int(30)));
    }

    #[test]
    fn hive_directory_names_are_filesystem_safe() {
        assert_eq!(hive_dir_name("region", "eu"), "region=eu");
        // Characters that cannot appear in a path component become _.
        assert_eq!(hive_dir_name("path", "a/b"), "path=a_b");
        assert_eq!(hive_dir_name("win", "c:\\d"), "win=c__d");
        // The engine's own null marker survives, so dataset mode reads it back.
        assert_eq!(hive_dir_name("region", "__null__"), "region=__null__");
        // Dots and dashes are legal in a Hive value and must be kept, or the
        // value could not be recovered from the directory name.
        assert_eq!(hive_dir_name("d", "2024-01-31"), "d=2024-01-31");
        assert_eq!(hive_dir_name("v", "1.2.3"), "v=1.2.3");
    }

    #[test]
    fn layout_parses_the_cli_words_and_nothing_else() {
        for l in PartitionLayout::ALL {
            assert_eq!(PartitionLayout::parse(l.id()), Some(*l), "{}", l.id());
            assert_eq!(PartitionLayout::parse(&l.id().to_uppercase()), Some(*l));
        }
        // An underscore is as natural as a hyphen on a command line.
        assert_eq!(
            PartitionLayout::parse("hive_parts"),
            Some(PartitionLayout::HiveParts)
        );
        assert_eq!(
            PartitionLayout::parse("  flat  "),
            Some(PartitionLayout::Flat)
        );
        assert_eq!(PartitionLayout::parse("nested"), None);
        assert_eq!(PartitionLayout::parse(""), None);
    }

    /// The one function every surface names files with. If these shapes
    /// change, the dialog preview, the writer, the CLI and the MCP tool all
    /// change together - which is the point of it being one function.
    #[test]
    fn partition_path_shapes() {
        let p = |l| partition_path(l, "city", "New York", "csv", 1);
        assert_eq!(p(PartitionLayout::Flat), "new_york.csv");
        assert_eq!(p(PartitionLayout::Folder), "New York/part-0001.csv");
        assert_eq!(p(PartitionLayout::Hive), "city=New York/data.csv");
        assert_eq!(p(PartitionLayout::HiveParts), "city=New York/part-0001.csv");
    }

    /// Only the folder layouts keep the value intact; Flat folds it down to a
    /// SQL-safe stem, which is the whole reason the other three exist.
    #[test]
    fn folder_layouts_keep_the_value_flat_does_not() {
        let v = "New-York 2024";
        assert_eq!(
            partition_path(PartitionLayout::Flat, "c", v, "csv", 1),
            "new_york_2024.csv"
        );
        assert_eq!(
            partition_path(PartitionLayout::Folder, "c", v, "csv", 1),
            "New-York 2024/part-0001.csv"
        );
    }

    /// A character that cannot appear in a path is replaced in every layout
    /// that puts the value in a directory name.
    #[test]
    fn path_illegal_characters_are_replaced_in_folder_names() {
        for l in [
            PartitionLayout::Folder,
            PartitionLayout::Hive,
            PartitionLayout::HiveParts,
        ] {
            let out = partition_path(l, "c", "a/b", "csv", 1);
            assert!(!out.starts_with("a/b"), "{l:?} left a raw slash: {out}");
            assert!(out.contains("a_b"), "{l:?}: {out}");
        }
    }

    /// Flat is the only lossy layout, so it is the only one that can collide.
    #[test]
    fn only_flat_needs_the_collision_counter() {
        let mut seen = std::collections::HashMap::new();
        let names: Vec<String> = ["New York", "new-york", "NEW_YORK"]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let rel = partition_path(PartitionLayout::Flat, "c", v, "csv", i + 1);
                dedupe_flat_name(PartitionLayout::Flat, rel, "csv", &mut seen)
            })
            .collect();
        assert_eq!(names, ["new_york.csv", "new_york_2.csv", "new_york_3.csv"]);

        let mut seen = std::collections::HashMap::new();
        let names: Vec<String> = ["New York", "new-york", "NEW_YORK"]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let rel = partition_path(PartitionLayout::Hive, "c", v, "csv", i + 1);
                dedupe_flat_name(PartitionLayout::Hive, rel, "csv", &mut seen)
            })
            .collect();
        assert_eq!(
            names,
            [
                "c=New York/data.csv",
                "c=new-york/data.csv",
                "c=NEW_YORK/data.csv"
            ],
            "already distinct, so no counter is applied"
        );
    }
}
