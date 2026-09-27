//! `octa --merge FILE FILE [FILE...] [--merge-original FILE] [--merge-key
//! COLS] [--merge-prefer N] [--merge-out FILE] [--merge-format EXT]` - merge
//! two or more versions of a table, optionally against their original.
//!
//! **Exits 1 and writes nothing while conflicts remain**, printing them on
//! stdout (one column per version). That is exactly the contract of a git
//! merge driver: `%A` keeps "ours", git marks the path conflicted, and the
//! GUI's Merge versions dialog reads the versions from the index.
//!
//! `--merge-format` exists because git hands a driver temp files with no
//! extension. It takes an extension (`csv`) or a file name to take it from
//! (git's `%P`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use octa::data::DataTable;
use octa::data::merge_versions::{conflict_table, merge_versions};
use octa::formats::FormatRegistry;

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub versions: Vec<PathBuf>,
    pub original: Option<PathBuf>,
    pub keys: Vec<String>,
    /// 1-based position in `versions`.
    pub prefer: Option<usize>,
    pub out: Option<PathBuf>,
    pub format: Option<String>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let ext = args.format.as_deref().map(ext_of);
    let read = |p: &Path| read_as(p, ext.as_deref());
    let original = args.original.as_deref().map(read).transpose()?;
    let versions: Vec<DataTable> = args
        .versions
        .iter()
        .map(|p| read(p))
        .collect::<anyhow::Result<_>>()?;
    if args.keys.is_empty() {
        eprintln!("note: no --merge-key given, so rows are matched by position");
    }
    let refs: Vec<&DataTable> = versions.iter().collect();
    let mut result = merge_versions(original.as_ref(), &refs, &args.keys)?;
    if let Some(n) = args.prefer {
        if n == 0 || n > versions.len() {
            anyhow::bail!("--merge-prefer takes 1 to {}", versions.len());
        }
        result.prefer_all(n - 1);
    }
    let open = result.unresolved();
    if open > 0 {
        write_table(&conflict_table(&result), format)?;
        eprintln!(
            "{open} conflict(s) need a decision; nothing written. Resolve them in \
             the GUI (File -> Merge versions...) or pass --merge-prefer N"
        );
        return Ok(ExitCode::FAILURE);
    }

    let merged = result.finish()?;
    match &args.out {
        Some(out) => {
            write_to(out, &merged, ext.as_deref())?;
            eprintln!(
                "merged {} rows x {} columns into {}",
                merged.row_count(),
                merged.col_count(),
                out.display()
            );
        }
        None => write_table(&merged, format)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// `csv`, `.csv` and `data/sales.csv` all mean csv.
fn ext_of(s: &str) -> String {
    Path::new(s)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| s.trim_start_matches('.').to_string())
}

/// The reader for `path`, or for `ext` when given: a reader never looks at
/// the name again once chosen, so a dummy file name picks it.
fn reader<'r>(
    registry: &'r FormatRegistry,
    path: &Path,
    ext: Option<&str>,
) -> anyhow::Result<&'r dyn octa::formats::FormatReader> {
    let probe = match ext {
        Some(ext) => PathBuf::from(format!("merge.{ext}")),
        None => path.to_path_buf(),
    };
    registry
        .reader_for_path(&probe)
        .ok_or_else(|| anyhow::anyhow!("no reader for {}", probe.display()))
}

fn read_as(path: &Path, ext: Option<&str>) -> anyhow::Result<DataTable> {
    match ext {
        Some(_) => reader(&FormatRegistry::new(), path, ext)?.read_file(path),
        None => super::read_table(path),
    }
}

fn write_to(path: &Path, table: &DataTable, ext: Option<&str>) -> anyhow::Result<()> {
    let registry = FormatRegistry::new();
    let writer = reader(&registry, path, ext)?;
    if !writer.supports_write() {
        anyhow::bail!("format {} does not support writing", writer.name());
    }
    writer.write_file(path, table)
}
