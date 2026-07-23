//! Directory-open path: Delta Lake / Apache Iceberg tables and plain datasets
//! of part files, read via DuckDB. Split out of `file_io/mod.rs`.

use crate::app::state::OctaApp;

impl OctaApp {
    /// Open a directory as a Delta Lake / Apache Iceberg table or as a plain
    /// dataset of part files. Detects the kind (marker subdirectory, else the
    /// data files inside) and reads it via DuckDB; reports a clear status
    /// message for empty directories or read failures (e.g. the
    /// `delta`/`iceberg` extension failing to install offline).
    pub(crate) fn load_lakehouse_dir(&mut self, path: std::path::PathBuf) {
        let Some(kind) = octa::formats::lakehouse_reader::detect(&path) else {
            self.status_message = Some((
                format!(
                    "No Delta Lake / Iceberg table or tabular part files in: {}",
                    path.display()
                ),
                std::time::Instant::now(),
            ));
            return;
        };
        match octa::formats::lakehouse_reader::read_dir_report(&path, kind) {
            Ok((table, skipped)) => {
                self.apply_loaded_table(path, table);
                if !skipped.is_empty() {
                    // Minority-family files were left out of the dataset scan;
                    // surface them in the dismissible load banner.
                    let shown: Vec<&str> = skipped.iter().take(5).map(|s| s.as_str()).collect();
                    let more = skipped.len().saturating_sub(shown.len());
                    let mut list = shown.join(", ");
                    if more > 0 {
                        list.push_str(&format!(" (+{more})"));
                    }
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.parse_error_banner = Some(
                            octa::i18n::t("dataset.skipped_files")
                                .replace("{n}", &skipped.len().to_string())
                                .replace("{files}", &list),
                        );
                    }
                }
            }
            Err(e) => {
                self.status_message = Some((
                    format!("Error reading {} table: {e}", kind.format_name()),
                    std::time::Instant::now(),
                ));
            }
        }
    }
}
