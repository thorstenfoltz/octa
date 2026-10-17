//! Shared PIVOT / UNPIVOT SQL builders for DuckDB's `PIVOT` / `UNPIVOT`
//! statements. Used by the GUI Pivot dialog (`src/app/dialogs/pivot.rs`) and
//! the MCP `pivot` tool so the two emit identical SQL. The active table is
//! always registered as `data` by the caller (`octa::sql::run_query`).

/// Quote a DuckDB identifier (double quotes, internal quotes doubled).
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Aggregate function for a PIVOT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotAgg {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

impl PivotAgg {
    pub const ALL: &'static [PivotAgg] = &[
        PivotAgg::Sum,
        PivotAgg::Count,
        PivotAgg::Avg,
        PivotAgg::Min,
        PivotAgg::Max,
    ];

    pub fn sql_fn(self) -> &'static str {
        match self {
            PivotAgg::Sum => "sum",
            PivotAgg::Count => "count",
            PivotAgg::Avg => "avg",
            PivotAgg::Min => "min",
            PivotAgg::Max => "max",
        }
    }

    /// Parse a case-insensitive aggregate name (`sum`/`count`/`avg`/`min`/`max`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sum" => Some(PivotAgg::Sum),
            "count" => Some(PivotAgg::Count),
            "avg" | "mean" | "average" => Some(PivotAgg::Avg),
            "min" => Some(PivotAgg::Min),
            "max" => Some(PivotAgg::Max),
            _ => None,
        }
    }
}

/// Build a `PIVOT data ON <on> USING <agg>(<value>) [GROUP BY <group...>]`
/// statement. Column names are quoted; `group` may be empty (DuckDB infers the
/// identity columns).
pub fn pivot_sql(on: &str, agg: PivotAgg, value: &str, group: &[String]) -> String {
    let mut sql = format!(
        "PIVOT data ON {} USING {}({})",
        quote_ident(on),
        agg.sql_fn(),
        quote_ident(value)
    );
    if !group.is_empty() {
        let groups: Vec<String> = group.iter().map(|g| quote_ident(g)).collect();
        sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
    }
    sql
}

/// Build an `UNPIVOT data ON <cols...> INTO NAME <name_col> VALUE <value_col>`
/// statement. Returns `None` when fewer than two columns are melted (UNPIVOT
/// needs at least two).
pub fn unpivot_sql(cols: &[String], name_col: &str, value_col: &str) -> Option<String> {
    if cols.len() < 2 {
        return None;
    }
    let melt: Vec<String> = cols.iter().map(|c| quote_ident(c)).collect();
    Some(format!(
        "UNPIVOT data ON {} INTO NAME {} VALUE {}",
        melt.join(", "),
        quote_ident(name_col.trim()),
        quote_ident(value_col.trim())
    ))
}

/// Plain-language description of a PIVOT, for the dialog's "what does this do"
/// line. Interpolates the chosen column names into a localized template; empty
/// names show as "?". Pure (only reads the i18n catalog).
pub fn explain_pivot(on: &str, agg: PivotAgg, value: &str, group: &[String]) -> String {
    let on = if on.is_empty() { "?" } else { on };
    let value = if value.is_empty() { "?" } else { value };
    if group.is_empty() {
        crate::i18n::t("dialog.pv_explain_pivot")
            .replace("{on}", on)
            .replace("{agg}", agg.sql_fn())
            .replace("{value}", value)
    } else {
        crate::i18n::t("dialog.pv_explain_pivot_grouped")
            .replace("{on}", on)
            .replace("{agg}", agg.sql_fn())
            .replace("{value}", value)
            .replace("{cols}", &group.join(", "))
    }
}

/// Plain-language description of an UNPIVOT for the dialog.
pub fn explain_unpivot(cols: &[String], name_col: &str, value_col: &str) -> String {
    let cols = if cols.is_empty() {
        "?".to_string()
    } else {
        cols.join(", ")
    };
    crate::i18n::t("dialog.pv_explain_unpivot")
        .replace("{cols}", &cols)
        .replace("{name}", name_col.trim())
        .replace("{value}", value_col.trim())
}

#[cfg(test)]
#[path = "pivot_tests.rs"]
mod tests;
