//! Pure parser + planner for bulk column rename from an old,new mapping.
//!
//! The dialog pastes (or loads) one `old,new` (or `old<TAB>new`) pair per line;
//! this module parses that text and, against the current column names, works out
//! which pairs match, which are unmatched, and which would collide, so the UI can
//! preview before applying.

/// The outcome of planning a bulk rename against a set of column names.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RenamePlan {
    /// `(col_index, old_name, new_name)` for each pair whose `old` exists and is
    /// not a collision.
    pub matched: Vec<(usize, String, String)>,
    /// `old` names that were not found among the current columns.
    pub unmatched: Vec<String>,
    /// Human-readable descriptions of collisions (target already exists, or two
    /// pairs target the same name). Non-empty blocks Apply in the UI.
    pub collisions: Vec<String>,
}

/// Parse mapping text into `(old, new)` pairs. Each non-blank line is split on
/// the first tab if present, else the first comma; both sides are trimmed and
/// only pairs with a non-empty `old` and `new` are kept.
pub fn parse_mapping(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let split_at = line.find('\t').or_else(|| line.find(','));
        let Some(idx) = split_at else {
            continue;
        };
        let (old, rest) = line.split_at(idx);
        // rest starts with the separator char; drop it.
        let new = &rest[1..];
        let old = old.trim();
        let new = new.trim();
        if old.is_empty() || new.is_empty() {
            continue;
        }
        pairs.push((old.to_string(), new.to_string()));
    }
    pairs
}

/// Plan a bulk rename of `columns` from `pairs`.
///
/// Collision rules:
/// - A pair whose target `new` equals an existing column that is *not* itself
///   being renamed away by another pair collides (the name would duplicate).
/// - Two matched pairs that target the same `new` collide.
pub fn plan_renames(columns: &[String], pairs: &[(String, String)]) -> RenamePlan {
    let mut plan = RenamePlan::default();

    // Names of columns that are the source (`old`) of some matched pair; these
    // are being vacated, so a target reusing them is not a collision.
    let renamed_away: std::collections::HashSet<&str> = pairs
        .iter()
        .filter(|(old, _)| columns.iter().any(|c| c == old))
        .map(|(old, _)| old.as_str())
        .collect();

    // Count how many pairs target each `new` (for duplicate-target detection).
    let mut target_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for (old, new) in pairs {
        if columns.iter().any(|c| c == old) {
            *target_counts.entry(new.as_str()).or_insert(0) += 1;
        }
    }

    for (old, new) in pairs {
        let Some(index) = columns.iter().position(|c| c == old) else {
            plan.unmatched.push(old.clone());
            continue;
        };

        // Target duplicated by two matched pairs.
        if target_counts.get(new.as_str()).copied().unwrap_or(0) > 1 {
            plan.collisions
                .push(format!("'{new}' is targeted by more than one rename"));
            continue;
        }

        // Target equals an existing column not being renamed away (and not the
        // column we are renaming, which is a harmless no-op / self-rename).
        let existing_conflict = columns
            .iter()
            .enumerate()
            .any(|(i, c)| c == new && i != index && !renamed_away.contains(c.as_str()));
        if existing_conflict {
            plan.collisions
                .push(format!("'{new}' already exists as a column"));
            continue;
        }

        plan.matched.push((index, old.clone(), new.clone()));
    }

    plan
}

/// Plan the renames that make every column name unique.
///
/// The first column carrying a name keeps it; every later one gets `_2`,
/// `_3`, ... appended, skipping any suffix that some *other* column already
/// owns, so de-colliding `a`, `a`, `a_2` renames only the middle one (to
/// `a_3`) instead of pushing the rename down the row.
///
/// With `ignore_case` the comparison folds case, so `Name` and `name` count
/// as the same name; the kept column's own spelling is never touched.
///
/// Returns `(col_index, old_name, new_name)` like [`RenamePlan::matched`],
/// addressed **by index** - which is the whole point, since the one thing a
/// name cannot identify is a column whose name is a duplicate.
pub fn plan_dedupe(columns: &[String], ignore_case: bool) -> Vec<(usize, String, String)> {
    let fold = |s: &str| {
        if ignore_case {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let owned: std::collections::HashSet<String> = columns.iter().map(|c| fold(c)).collect();
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut renames = Vec::new();
    for (index, name) in columns.iter().enumerate() {
        if taken.insert(fold(name)) {
            continue;
        }
        let new = crate::data::unique_name(name, |c| {
            let key = fold(c);
            taken.contains(&key) || owned.contains(&key)
        });
        taken.insert(fold(&new));
        renames.push((index, name.clone(), new));
    }
    renames
}

#[cfg(test)]
#[path = "rename_map_tests.rs"]
mod tests;
