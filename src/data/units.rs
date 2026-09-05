//! Numbers wearing a unit: `1.2k`, `EUR 4,00`, `12 kg`, `45%`.
//!
//! These are text to every reader, so they sort alphabetically, do not sum,
//! and quietly poison any average taken over them. Splitting the number out is
//! easy; the part that matters is that **nothing here changes a cell**. This
//! module only reads: it says what a column looks like and what the split
//! would produce, and the dialog above it does the changing, once, on Apply,
//! through the normal undo path.
//!
//! **No second number parser.** The numeric core goes through
//! `num_parse::parse_number_relaxed`, which already knows that `1.234,56` is
//! European and `1,234.56` is English. This is that idea one layer up: peel
//! off the decoration, hand the rest to the parser that exists.

use std::collections::HashMap;

use crate::data::num_parse::{
    NumberOutcome, NumberStyle, infer_column, parse_number, parse_number_relaxed,
};

/// A column must be this uniform before it is worth suggesting anything. Below
/// it the column is prose that happens to contain numbers.
pub const MIN_SHARE: f64 = 0.8;

/// And this many values have to parse at all, so a three-row table does not
/// produce a confident-looking finding.
pub const MIN_MATCHES: usize = 3;

/// What kind of decoration the column carries. Only used to phrase the
/// suggestion; the split works the same way for all four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flavour {
    /// A magnitude suffix folded into the number: `1.2k` is 1200.
    Magnitude,
    /// A currency symbol or code: `EUR 4,00`, `$1,200`.
    Currency,
    /// A measurement unit: `12 kg`, `5 ms`.
    Unit,
    /// A percentage. The number is kept as written: `45%` is 45, not 0.45.
    Percent,
}

impl Flavour {
    pub fn id(self) -> &'static str {
        match self {
            Self::Magnitude => "magnitude",
            Self::Currency => "currency",
            Self::Unit => "unit",
            Self::Percent => "percent",
        }
    }
}

/// One value taken apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The number, with any magnitude suffix already applied.
    pub number: f64,
    /// What was attached to it, or empty for a bare magnitude (`1.2k` is a
    /// number, not a number of anything).
    pub unit: String,
    pub flavour: Flavour,
}

/// What a whole column looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub flavour: Flavour,
    /// The unit that dominates the column, for the suggestion's wording.
    /// Empty for a magnitude column.
    pub unit: String,
    /// Values that parsed.
    pub matched: usize,
    /// Non-empty values seen.
    pub total: usize,
    /// Whether every parsed value carried the same unit. A column mixing `kg`
    /// and `lb` is exactly the one nobody should sum, so the caller says so
    /// rather than hiding it.
    pub mixed_units: bool,
    /// The decimal convention the whole column reads in, once its numeric
    /// cores have been looked at together. `None` when they do not settle it.
    ///
    /// This is why detection is not `parse_value` in a loop. On its own
    /// `$1,200` is genuinely undecidable - twelve hundred dollars in Ohio, one
    /// euro twenty in Bavaria - and a per-value parser has to pick one. A
    /// *column* of them usually is decidable, and picking wrong is wrong by a
    /// factor of a thousand.
    pub style: Option<NumberStyle>,
}

/// Magnitude suffixes, longest first so `mio` is tried before `m`.
const MAGNITUDES: &[(&str, f64)] = &[
    ("mrd", 1e9),
    ("mio", 1e6),
    ("bn", 1e9),
    ("mn", 1e6),
    ("m", 1e6),
    ("k", 1e3),
    ("b", 1e9),
];

/// Currency symbols and the three-letter codes worth recognising without a
/// table of every ISO code, which would start matching ordinary words.
const CURRENCIES: &[&str] = &[
    "$", "EUR", "USD", "GBP", "CHF", "JPY", "SEK", "NOK", "DKK", "PLN", "CZK", "CAD", "AUD",
];

/// Take one value apart, letting the value pick its own decimal convention.
///
/// Prefer [`parse_value_with`] and the style [`detect_column`] resolved: on its
/// own this cannot tell `$1,200` in Ohio from `$1,200` in Bavaria.
pub fn parse_value(raw: &str) -> Option<Parsed> {
    parse_value_with(raw, None)
}

/// Take one value apart, reading its number in `style` when one is known.
pub fn parse_value_with(raw: &str, style: Option<NumberStyle>) -> Option<Parsed> {
    let (core, unit, flavour) = peel(raw)?;
    let number = number(&core, style)?;
    Some(Parsed {
        number: number * magnitude_factor(&unit, flavour),
        unit: if flavour == Flavour::Magnitude {
            String::new()
        } else {
            unit
        },
        flavour,
    })
}

/// What a magnitude suffix multiplies by; 1.0 for everything else.
fn magnitude_factor(unit: &str, flavour: Flavour) -> f64 {
    if flavour != Flavour::Magnitude {
        return 1.0;
    }
    MAGNITUDES
        .iter()
        .find(|(m, _)| *m == unit)
        .map(|(_, f)| *f)
        .unwrap_or(1.0)
}

fn number(core: &str, style: Option<NumberStyle>) -> Option<f64> {
    match style {
        // A column-wide style still has to admit a plain `12`, which
        // `parse_number` alone rejects for the style it was not written in.
        Some(s) => parse_number(core, s).or_else(|| core.trim().parse::<f64>().ok()),
        None => parse_number_relaxed(core),
    }
}

/// Strip the decoration off a value: the numeric core, what was attached, and
/// which kind of thing it is. **Parses no number**, so the caller can settle
/// the decimal convention across the whole column first.
fn peel(raw: &str) -> Option<(String, String, Flavour)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // A plain number is not this feature's business: the number-inference pass
    // already promotes those, and claiming them here would put a second,
    // competing suggestion in front of the user.
    if parse_number_relaxed(s).is_some() {
        return None;
    }

    if let Some(core) = s.strip_suffix('%')
        && parse_number_relaxed(core.trim()).is_some()
    {
        return Some((core.trim().to_string(), "%".to_string(), Flavour::Percent));
    }

    // A currency can sit on either side, and the symbol may touch the digits.
    for cur in CURRENCIES {
        for core in [strip_prefix_ci(s, cur), strip_suffix_ci(s, cur)]
            .into_iter()
            .flatten()
        {
            let core = core.trim();
            if parse_number_relaxed(core).is_some() {
                return Some((core.to_string(), (*cur).to_string(), Flavour::Currency));
            }
        }
    }

    // Everything left splits into a numeric head and an alphabetic tail.
    let (head, tail) = split_head_tail(s)?;
    parse_number_relaxed(head)?;
    let tail_lc = tail.to_ascii_lowercase();
    if MAGNITUDES.iter().any(|(m, _)| *m == tail_lc) {
        return Some((head.to_string(), tail_lc, Flavour::Magnitude));
    }
    Some((head.to_string(), tail.to_string(), Flavour::Unit))
}

/// Split `12 kg` into `("12", "kg")`. The tail has to be letters, so `12-14`
/// and `2024-01-02` are not units with a strange name.
fn split_head_tail(s: &str) -> Option<(&str, &str)> {
    let cut = s
        .char_indices()
        .find(|(_, c)| c.is_ascii_alphabetic())
        .map(|(i, _)| i)?;
    let (head, tail) = s.split_at(cut);
    let tail = tail.trim();
    if head.trim().is_empty() || tail.is_empty() || !tail.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((head.trim(), tail))
}

fn strip_prefix_ci<'a>(s: &'a str, pat: &'a str) -> Option<&'a str> {
    (s.len() >= pat.len() && s[..pat.len()].eq_ignore_ascii_case(pat)).then(|| &s[pat.len()..])
}

fn strip_suffix_ci<'a>(s: &'a str, pat: &'a str) -> Option<&'a str> {
    (s.len() >= pat.len() && s[s.len() - pat.len()..].eq_ignore_ascii_case(pat))
        .then(|| &s[..s.len() - pat.len()])
}

/// What a column of text looks like, or `None` when it is not worth
/// mentioning.
pub fn detect_column(values: &[Option<&str>]) -> Option<Detection> {
    let mut total = 0usize;
    let mut flavours: HashMap<Flavour, usize> = HashMap::new();
    let mut units: HashMap<String, usize> = HashMap::new();
    let mut cores: Vec<String> = Vec::new();

    for v in values.iter().flatten() {
        if v.trim().is_empty() {
            continue;
        }
        total += 1;
        if let Some((core, unit, flavour)) = peel(v) {
            *flavours.entry(flavour).or_insert(0) += 1;
            let unit = if flavour == Flavour::Magnitude {
                String::new()
            } else {
                unit
            };
            *units.entry(unit).or_insert(0) += 1;
            cores.push(core);
        }
    }
    let matched = cores.len();

    // The decimal convention is settled over the whole column, not per value.
    // See `Detection::style`.
    let refs: Vec<Option<&str>> = cores.iter().map(|c| Some(c.as_str())).collect();
    let style = match infer_column(&refs) {
        NumberOutcome::Promote(s) => Some(s),
        NumberOutcome::Ambiguous | NumberOutcome::Skip => None,
    };

    if matched < MIN_MATCHES || total == 0 || (matched as f64 / total as f64) < MIN_SHARE {
        return None;
    }
    // Ties broken by the id so the answer does not depend on HashMap order.
    let flavour = *flavours
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.id().cmp(a.0.id())))?
        .0;
    let unit = units
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(u, _)| u.clone())
        .unwrap_or_default();

    Some(Detection {
        flavour,
        unit,
        matched,
        total,
        mixed_units: units.len() > 1,
        style,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Option<f64> {
        parse_value(s).map(|p| p.number)
    }

    #[test]
    fn magnitude_suffixes_fold_into_the_number() {
        assert_eq!(n("1.2k"), Some(1200.0));
        assert_eq!(n("3 Mio"), Some(3_000_000.0));
        assert_eq!(n("2M"), Some(2_000_000.0));
        assert_eq!(n("4 mrd"), Some(4e9));
        assert_eq!(parse_value("1.2k").unwrap().unit, "");
        assert_eq!(parse_value("1.2k").unwrap().flavour, Flavour::Magnitude);
    }

    #[test]
    fn a_currency_is_recognised_on_either_side() {
        assert_eq!(n("EUR 4,00"), Some(4.0));
        assert_eq!(n("4,00 EUR"), Some(4.0));
        assert_eq!(n("$1,200.50"), Some(1200.50));
        assert_eq!(parse_value("$1,200.50").unwrap().unit, "$");
        assert_eq!(parse_value("EUR 4,00").unwrap().flavour, Flavour::Currency);
    }

    /// `$1,200` on its own is genuinely undecidable: twelve hundred dollars in
    /// Ohio, one euro twenty in Bavaria. A per-value read has to guess, and a
    /// wrong guess here is wrong by a factor of a thousand - which is why
    /// `detect_column` settles the convention over the whole column and hands
    /// it back for the apply step to use.
    #[test]
    fn a_lone_thousands_group_is_undecidable_but_a_column_of_them_is_not() {
        // Alone: the relaxed parser falls back to the European reading.
        assert_eq!(n("$1,200"), Some(1.2));

        // In a column where another value settles it, the style comes back and
        // parsing with it gives the right number.
        let d = detect_column(&col(&["$1,200", "$3,400", "$1,234.56"])).expect("detected");
        assert_eq!(d.style, Some(NumberStyle::English));
        assert_eq!(
            parse_value_with("$1,200", d.style).map(|p| p.number),
            Some(1200.0)
        );

        // And the other way round, where the column is European.
        let d = detect_column(&col(&["EUR 1,20", "EUR 3,40", "EUR 1.234,56"])).expect("detected");
        assert_eq!(d.style, Some(NumberStyle::European));
        assert_eq!(
            parse_value_with("EUR 1,20", d.style).map(|p| p.number),
            Some(1.2)
        );
    }

    /// The whole reason this reuses `num_parse`: `4,00` is four euros in
    /// Germany and `1,200` is twelve hundred dollars in the States, and one
    /// hand-rolled parser here would get one of them wrong.
    #[test]
    fn european_and_english_decimals_both_work() {
        assert_eq!(n("1.234,56 EUR"), Some(1234.56));
        assert_eq!(n("1,234.56 USD"), Some(1234.56));
    }

    #[test]
    fn units_keep_their_name() {
        assert_eq!(n("12 kg"), Some(12.0));
        assert_eq!(parse_value("12 kg").unwrap().unit, "kg");
        assert_eq!(parse_value("5ms").unwrap().unit, "ms");
        assert_eq!(parse_value("5ms").unwrap().flavour, Flavour::Unit);
    }

    /// Kept as written. Turning 45% into 0.45 is a change of meaning, and this
    /// module does not change meanings.
    #[test]
    fn a_percentage_keeps_its_number() {
        assert_eq!(n("45%"), Some(45.0));
        assert_eq!(parse_value("45 %").unwrap().unit, "%");
        assert_eq!(parse_value("45%").unwrap().flavour, Flavour::Percent);
    }

    /// A bare number belongs to the number-inference pass. Claiming it here
    /// would put two competing suggestions in front of the user.
    #[test]
    fn a_plain_number_is_not_this_features_business() {
        assert_eq!(parse_value("42"), None);
        assert_eq!(parse_value("1.234,56"), None);
        assert_eq!(parse_value("-3.5"), None);
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        for s in ["hello", "", "  ", "2024-01-02", "12-14", "N/A", "a1"] {
            assert_eq!(parse_value(s), None, "{s:?} should not parse");
        }
    }

    fn col<'a>(values: &[&'a str]) -> Vec<Option<&'a str>> {
        values.iter().map(|v| Some(*v)).collect()
    }

    #[test]
    fn a_uniform_column_is_detected() {
        let d = detect_column(&col(&["12 kg", "3 kg", "40 kg", "7 kg"])).expect("detected");
        assert_eq!(d.flavour, Flavour::Unit);
        assert_eq!(d.unit, "kg");
        assert_eq!(d.matched, 4);
        assert_eq!(d.total, 4);
        assert!(!d.mixed_units);
    }

    /// The column nobody should sum, said out loud rather than hidden.
    #[test]
    fn mixed_units_are_reported_as_mixed() {
        let d = detect_column(&col(&["12 kg", "3 lb", "40 kg", "7 lb"])).expect("detected");
        assert!(d.mixed_units);
    }

    #[test]
    fn prose_with_a_number_in_it_is_not_a_unit_column() {
        assert_eq!(
            detect_column(&col(&["12 kg", "see note", "ask Bob", "n/a", "later"])),
            None
        );
    }

    #[test]
    fn a_tiny_column_is_not_enough_to_go_on() {
        assert_eq!(detect_column(&col(&["12 kg", "3 kg"])), None);
    }

    #[test]
    fn a_column_of_plain_numbers_is_not_detected() {
        assert_eq!(detect_column(&col(&["1", "2", "3", "4"])), None);
    }
}
