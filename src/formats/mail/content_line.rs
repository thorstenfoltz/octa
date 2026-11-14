//! iCalendar (RFC 5545) and vCard (RFC 6350, plus 2.1 as phones and Outlook
//! export it) share one line format: `NAME;PARAM=VALUE:value`, long lines
//! folded with a leading space, and in 2.1 quoted-printable with `=` soft
//! breaks.

/// Logical lines: folded lines joined, quoted-printable soft breaks joined.
pub fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut qp_open = false;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if qp_open && let Some(last) = out.last_mut() {
            last.pop(); // the '=' soft break
            last.push_str(line);
            qp_open = line.ends_with('=');
            continue;
        }
        if let Some(rest) = line.strip_prefix([' ', '\t'])
            && let Some(last) = out.last_mut()
        {
            last.push_str(rest);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        qp_open = line.to_ascii_uppercase().contains("QUOTED-PRINTABLE") && line.ends_with('=');
        out.push(line.to_string());
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    /// Upper case, group prefix (`item1.`) removed.
    pub name: String,
    /// Upper-case keys. vCard 2.1's bare `TEL;WORK;VOICE` become one `TYPE`.
    pub params: Vec<(String, String)>,
    pub value: String,
}

/// Split at the first `:` outside double quotes.
pub fn parse(line: &str) -> Option<Prop> {
    let mut in_quotes = false;
    let colon = line
        .char_indices()
        .find(|(_, c)| {
            if *c == '"' {
                in_quotes = !in_quotes;
            }
            *c == ':' && !in_quotes
        })?
        .0;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_params(head).into_iter();
    let name = parts.next()?.to_ascii_uppercase();
    let name = name.rsplit('.').next().unwrap_or(&name).to_string();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut bare: Vec<String> = Vec::new();
    for p in parts {
        match p.split_once('=') {
            Some((k, v)) => params.push((k.to_ascii_uppercase(), v.trim_matches('"').to_string())),
            None => bare.push(p.to_ascii_uppercase()),
        }
    }
    if !bare.is_empty() {
        params.push(("TYPE".into(), bare.join(",")));
    }
    Some(Prop {
        name,
        params,
        value: value.to_string(),
    })
}

fn split_params(head: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    for c in head.chars() {
        match c {
            '"' => {
                q = !q;
                cur.push(c);
            }
            ';' if !q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

impl Prop {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The value with quoted-printable and charset decoded but escapes kept:
    /// what a structured value (`N`, `ADR`) is split from.
    pub fn raw(&self) -> String {
        let qp = self
            .param("ENCODING")
            .is_some_and(|e| e.eq_ignore_ascii_case("QUOTED-PRINTABLE"));
        if qp {
            decode_qp(&self.value, self.param("CHARSET"))
        } else {
            self.value.clone()
        }
    }

    /// The value as text: quoted-printable and charset decoded, escapes undone.
    pub fn text(&self) -> String {
        let raw = self.raw();
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n' | 'N') => out.push('\n'),
                    Some(other) => out.push(other),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}

fn decode_qp(s: &str, charset: Option<&str>) -> String {
    let b = s.as_bytes();
    let mut bytes = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'='
            && i + 2 < b.len()
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            bytes.push(v);
            i += 3;
            continue;
        }
        bytes.push(b[i]);
        i += 1;
    }
    let enc = charset
        .and_then(|c| encoding_rs::Encoding::for_label(c.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(&bytes).0.into_owned()
}

/// A DTSTART/DTEND/DUE value as `YYYY-MM-DD HH:MM:SS` and whether it is a
/// whole day. Zoned times are shown as written; `Z` times are UTC.
pub fn ics_time(p: &Prop) -> (Option<String>, bool) {
    let v = p.value.trim().trim_end_matches('Z');
    if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y%m%d") {
        return (Some(format!("{} 00:00:00", d.format("%Y-%m-%d"))), true);
    }
    let t = chrono::NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok();
    (t.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()), false)
}

#[cfg(test)]
#[path = "content_line_tests.rs"]
mod tests;
