//! A Markdown document as a printed PDF.
//!
//! A Markdown tab holds its file as a one-column table of source lines, so the
//! table export printed the raw `#`, `**` and `|` syntax as table rows. This
//! lays the document out the way the preview shows it instead: headings,
//! paragraphs with bold, italic, code and links, lists, quotes, code blocks,
//! tables and rules, wrapped to the page and paginated. It reads the same
//! pulldown-cmark events with the same options as the preview
//! (`view_modes::markdown::render_pulldown`), so both agree on what the
//! document says, and the pages go out through the table export's
//! SVG -> PDF path with the bundled fonts, measured by their own advances.

use std::fmt::Write as _;

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::chart_export::escape_xml;
use super::pdf_export::{Face, PdfOptions, char_width, pages_to_pdf, text_width_in};

const MARGIN: f32 = 54.0;
const BODY: f32 = 10.5;
const CODE: f32 = 8.8;
const LEADING: f32 = 1.45;
const CODE_LEADING: f32 = 1.4;
const FOOTER_FONT: f32 = 7.0;
const GAP_PARAGRAPH: f32 = 7.0;
const GAP_BLOCK: f32 = 9.0;
const GAP_ITEM: f32 = 2.0;
const GAP_BEFORE_HEADING: f32 = 12.0;
const GAP_AFTER_HEADING: f32 = 5.0;
/// Indent per list level; the marker sits in the first `LIST_INDENT` points.
const LIST_INDENT: f32 = 18.0;
const QUOTE_INDENT: f32 = 12.0;
const QUOTE_BAR: f32 = 2.5;
const CODE_PAD: f32 = 5.0;
const CELL_PAD: f32 = 4.0;
/// Italic by slanting the upright face: no italic face is bundled, and a
/// system one would be measured with the wrong advances.
const ITALIC_SKEW: f32 = -12.0;

const INK: &str = "#1f2328";
const MUTED: &str = "#59636e";
const LINK: &str = "#0969da";
const LINE: &str = "#d1d9e0";
const CODE_BG: &str = "#f2f4f6";
const HEAD_BG: &str = "#f6f8fa";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: bool,
}

impl Style {
    fn face(self) -> Face {
        if self.code {
            Face::Mono
        } else if self.bold {
            Face::Bold
        } else {
            Face::Regular
        }
    }
}

/// Inline text as the parser hands it over.
#[derive(Debug, Clone)]
struct Run {
    text: String,
    style: Style,
}

/// Text placed on a line; `x` is from the line's left edge.
#[derive(Debug, Clone)]
struct Piece {
    x: f32,
    w: f32,
    text: String,
    style: Style,
    size: f32,
}

type Line = Vec<Piece>;

fn line_width(line: &Line) -> f32 {
    line.last().map_or(0.0, |p| p.x + p.w)
}

/// Break `runs` into lines no wider than `width`, at spaces, and inside a
/// word only when the word alone is wider than the line. `\n` is a hard
/// break. Neighbouring words in one style become one piece.
fn wrap(runs: &[Run], size: f32, width: f32, force_bold: bool) -> Vec<Line> {
    let mut lines: Vec<Line> = vec![Vec::new()];
    let mut x = 0.0_f32;
    // A space waits until the next word shows it is not at a line end.
    let mut space = 0.0_f32;
    for run in runs {
        let mut style = run.style;
        style.bold |= force_bold;
        let size = if style.code { size * 0.92 } else { size };
        let face = style.face();
        for (i, para) in run.text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
                x = 0.0;
                space = 0.0;
            }
            let mut rest = para;
            while !rest.is_empty() {
                let gap_len = rest.len() - rest.trim_start().len();
                if gap_len > 0 {
                    space = text_width_in(face, " ", size);
                    rest = &rest[gap_len..];
                    continue;
                }
                let word_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let mut word = &rest[..word_len];
                rest = &rest[word_len..];
                loop {
                    let lead = if x > 0.0 { space } else { 0.0 };
                    let w = text_width_in(face, word, size);
                    if x > 0.0 && x + lead + w > width {
                        lines.push(Vec::new());
                        x = 0.0;
                        continue;
                    }
                    // Wider than a whole line: cut it where the line ends.
                    let (head, tail) = if w > width - x - lead {
                        split_at_width(word, face, size, width - x - lead)
                    } else {
                        (word, "")
                    };
                    let line = lines.last_mut().expect("there is always a line");
                    let hw = text_width_in(face, head, size);
                    place(line, x + lead, hw, head, style, size, lead > 0.0);
                    x += lead + hw;
                    space = 0.0;
                    if tail.is_empty() {
                        break;
                    }
                    word = tail;
                    lines.push(Vec::new());
                    x = 0.0;
                }
            }
        }
    }
    if lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// The longest head of `word` that fits `width` (at least one character, so
/// a line always makes progress), and the rest.
fn split_at_width(word: &str, face: Face, size: f32, width: f32) -> (&str, &str) {
    let mut used = 0.0;
    let mut cut = word.len();
    for (i, c) in word.char_indices() {
        used += char_width(face, c, size);
        if used > width && i > 0 {
            cut = i;
            break;
        }
    }
    word.split_at(cut)
}

/// Append a word to the line, joining it to the piece before when that piece
/// has the same style and size and ends exactly where the space began.
fn place(line: &mut Line, x: f32, w: f32, text: &str, style: Style, size: f32, spaced: bool) {
    if let Some(last) = line.last_mut()
        && last.style == style
        && last.size == size
    {
        if spaced {
            last.text.push(' ');
        }
        last.text.push_str(text);
        last.w = x + w - last.x;
        return;
    }
    line.push(Piece {
        x,
        w,
        text: text.to_string(),
        style,
        size,
    });
}

/// The pages being written: finished page bodies plus the current one.
struct Doc {
    pw: f32,
    ph: f32,
    pages: Vec<String>,
    body: String,
    /// Top of the next thing on the current page.
    y: f32,
    /// Open block quotes, so a page break or a gap keeps drawing their bars.
    quote: usize,
}

impl Doc {
    fn new(opts: &PdfOptions) -> Self {
        let (pw, ph) = opts.page_points();
        Doc {
            pw,
            ph,
            pages: Vec::new(),
            body: String::new(),
            y: MARGIN,
            quote: 0,
        }
    }

    fn left(&self) -> f32 {
        MARGIN
    }

    fn right(&self) -> f32 {
        self.pw - MARGIN
    }

    fn bottom(&self) -> f32 {
        self.ph - MARGIN
    }

    fn at_top(&self) -> bool {
        self.y <= MARGIN
    }

    /// Start a new page unless `h` more points fit on this one. Something
    /// taller than a page starts on a fresh one and runs over its margin.
    fn room(&mut self, h: f32) {
        if self.y + h > self.bottom() && !self.at_top() {
            self.pages.push(std::mem::take(&mut self.body));
            self.y = MARGIN;
        }
    }

    /// Vertical space, never at the top of a page. Inside a quote the bars
    /// run on through it.
    fn gap(&mut self, h: f32) {
        if self.at_top() {
            return;
        }
        let h = h.min(self.bottom() - self.y).max(0.0);
        self.bars(h);
        self.y += h;
    }

    /// The bars of the open quotes beside a band `h` points tall at `y`.
    fn bars(&mut self, h: f32) {
        for level in 0..self.quote {
            let x = self.left() + level as f32 * QUOTE_INDENT;
            let _ = write!(
                self.body,
                r#"<rect x="{x:.2}" y="{y:.2}" width="{QUOTE_BAR}" height="{h:.2}" fill="{LINE}"/>"#,
                y = self.y,
            );
        }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: &str) {
        let _ = write!(
            self.body,
            r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{fill}"/>"#
        );
    }

    fn hline(&mut self, x1: f32, x2: f32, y: f32, width: f32) {
        let _ = write!(
            self.body,
            r#"<line x1="{x1:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}" stroke="{LINE}" stroke-width="{width}"/>"#
        );
    }

    /// Draw one line's pieces with their left edge at `x0` and their
    /// baseline at `baseline`. `ink` colours everything that is not a link.
    fn pieces(&mut self, line: &Line, x0: f32, baseline: f32, ink: &str) {
        for p in line {
            let x = x0 + p.x;
            if p.style.code {
                self.rect(
                    x - 1.5,
                    baseline - p.size * 0.85,
                    p.w + 3.0,
                    p.size * 1.15,
                    CODE_BG,
                );
            }
            let fill = if p.style.link { LINK } else { ink };
            let place = if p.style.italic {
                format!(
                    r#"x="0" y="0" transform="translate({x:.2} {baseline:.2}) skewX({ITALIC_SKEW})""#
                )
            } else {
                format!(r#"x="{x:.2}" y="{baseline:.2}""#)
            };
            let _ = write!(
                self.body,
                r#"<text {place} {face} font-size="{size:.2}" fill="{fill}" xml:space="preserve">{t}</text>"#,
                face = p.style.face().svg_attrs(),
                size = p.size,
                t = escape_xml(&p.text),
            );
            if p.style.link {
                let w = p.w;
                let y = baseline + p.size * 0.12;
                let _ = write!(
                    self.body,
                    r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="0.6" fill="{LINK}"/>"#
                );
            }
            if p.style.strike {
                let w = p.w;
                let y = baseline - p.size * 0.3;
                let _ = write!(
                    self.body,
                    r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="0.7" fill="{ink}"/>"#
                );
            }
        }
    }

    /// Lay `lines` down at `x0`, one line box `size * leading` tall each,
    /// breaking pages between lines. `first` runs once, on the first line's
    /// baseline (a list marker).
    fn lines(
        &mut self,
        lines: &[Line],
        x0: f32,
        size: f32,
        ink: &str,
        mut first: impl FnMut(&mut Doc, f32),
    ) {
        let h = size * LEADING;
        for (i, line) in lines.iter().enumerate() {
            self.room(h);
            self.bars(h);
            let baseline = self.y + h * 0.5 + size * 0.35;
            if i == 0 {
                first(self, baseline);
            }
            self.pieces(line, x0, baseline, ink);
            self.y += h;
        }
    }

    /// Every page, finished: white paper, the body, and the footer naming the
    /// document and the page.
    fn finish(mut self, title: &str) -> Vec<String> {
        if !self.body.is_empty() || self.pages.is_empty() {
            self.pages.push(std::mem::take(&mut self.body));
        }
        let total = self.pages.len();
        let (pw, ph) = (self.pw, self.ph);
        self.pages
            .iter()
            .enumerate()
            .map(|(i, body)| {
                let footer = if title.is_empty() {
                    format!("page {} of {total}", i + 1)
                } else {
                    format!("{title}  -  page {} of {total}", i + 1)
                };
                format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="0 0 {pw} {ph}"><rect width="{pw}" height="{ph}" fill="#ffffff"/>{body}<text x="{MARGIN}" y="{fy:.2}" {face} font-size="{FOOTER_FONT}" fill="{MUTED}">{t}</text></svg>"##,
                    fy = ph - MARGIN / 2.0,
                    face = Face::Regular.svg_attrs(),
                    t = escape_xml(&footer),
                )
            })
            .collect()
    }
}

/// A list being walked: `next` is the number of the next ordered item.
struct List {
    next: Option<u64>,
}

/// A table being collected from its events, then laid out whole, since a
/// column's width depends on every row.
struct TableBuf {
    aligns: Vec<Alignment>,
    head: Vec<Vec<Run>>,
    rows: Vec<Vec<Vec<Run>>>,
    row: Vec<Vec<Run>>,
    cell: Vec<Run>,
}

#[derive(Clone, Copy, PartialEq)]
enum Block {
    Paragraph,
    Heading(HeadingLevel),
    Code,
}

fn heading_size(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H1 => 20.0,
        HeadingLevel::H2 => 16.0,
        HeadingLevel::H3 => 13.5,
        HeadingLevel::H4 => 12.0,
        HeadingLevel::H5 => 11.0,
        HeadingLevel::H6 => BODY,
    }
}

/// Where the text of the current block starts: past the open quotes and lists.
fn indent(doc: &Doc, lists: usize) -> f32 {
    doc.left()
        + doc.quote as f32 * QUOTE_INDENT
        + if doc.quote > 0 { 4.0 } else { 0.0 }
        + lists as f32 * LIST_INDENT
}

/// Lay out the buffered inline text of the block that just ended.
fn flush(doc: &mut Doc, runs: &mut Vec<Run>, block: Block, lists: &[List], marker: &mut bool) {
    if runs.is_empty() {
        return;
    }
    let runs = std::mem::take(runs);
    let x0 = indent(doc, lists.len());
    let width = (doc.right() - x0).max(40.0);
    let ink = if doc.quote > 0 { MUTED } else { INK };
    match block {
        Block::Heading(level) => {
            let size = heading_size(level);
            let lines = wrap(&runs, size, width, true);
            doc.gap(GAP_BEFORE_HEADING);
            // Keep a heading with the first lines of its section.
            doc.room(lines.len() as f32 * size * LEADING + 2.0 * BODY * LEADING);
            let ink = if level == HeadingLevel::H6 {
                MUTED
            } else {
                ink
            };
            doc.lines(&lines, x0, size, ink, |_, _| {});
            if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                doc.y += 2.0;
                let right = doc.right();
                doc.hline(x0, right, doc.y, 0.8);
                doc.y += 2.0;
            }
            doc.y += GAP_AFTER_HEADING;
        }
        _ => {
            let lines = wrap(&runs, BODY, width, false);
            let bullet = std::mem::take(marker).then(|| lists.last()).flatten();
            let depth = lists.len();
            doc.lines(&lines, x0, BODY, ink, |doc, baseline| {
                if let Some(list) = bullet {
                    draw_marker(doc, list, depth, x0, baseline);
                }
            });
        }
    }
}

/// A list item's marker in the indent left of its text: the number of an
/// ordered item, else a dot, a ring or a square by depth.
fn draw_marker(doc: &mut Doc, list: &List, depth: usize, text_x: f32, baseline: f32) {
    match list.next {
        Some(n) => {
            let label = format!("{n}.");
            let x = text_x - 5.0 - text_width_in(Face::Regular, &label, BODY);
            let _ = write!(
                doc.body,
                r#"<text x="{x:.2}" y="{baseline:.2}" {face} font-size="{BODY}" fill="{INK}">{label}</text>"#,
                face = Face::Regular.svg_attrs(),
            );
        }
        None => {
            let (cx, cy) = (text_x - 9.0, baseline - BODY * 0.33);
            let shape = match depth {
                1 => format!(r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="1.9" fill="{INK}"/>"#),
                2 => format!(
                    r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="1.7" fill="none" stroke="{INK}" stroke-width="0.7"/>"#
                ),
                _ => format!(
                    r#"<rect x="{x:.2}" y="{y:.2}" width="3.2" height="3.2" fill="{INK}"/>"#,
                    x = cx - 1.6,
                    y = cy - 1.6
                ),
            };
            doc.body.push_str(&shape);
        }
    }
}

/// A fenced or indented code block: monospace on a tinted band, lines kept as
/// written and cut only where they run past the page.
fn code_block(doc: &mut Doc, code: &str, lists: usize) {
    let x0 = indent(doc, lists);
    let width = (doc.right() - x0 - CODE_PAD * 2.0).max(40.0);
    let mut lines: Vec<String> = Vec::new();
    for src in code.trim_end_matches('\n').split('\n') {
        let src = src.replace('\t', "    ");
        let mut rest = src.as_str();
        loop {
            let (head, tail) = split_at_width(rest, Face::Mono, CODE, width);
            lines.push(head.to_string());
            if tail.is_empty() {
                break;
            }
            rest = tail;
        }
    }
    let h = CODE * CODE_LEADING;
    let band = doc.right() - x0;
    doc.room(CODE_PAD + h);
    doc.rect(x0, doc.y, band, CODE_PAD, CODE_BG);
    doc.y += CODE_PAD;
    for line in &lines {
        doc.room(h);
        doc.bars(h);
        doc.rect(x0, doc.y, band, h, CODE_BG);
        let _ = write!(
            doc.body,
            r#"<text x="{x:.2}" y="{baseline:.2}" {face} font-size="{CODE}" fill="{INK}" xml:space="preserve">{t}</text>"#,
            x = x0 + CODE_PAD,
            baseline = doc.y + h * 0.5 + CODE * 0.35,
            face = Face::Mono.svg_attrs(),
            t = escape_xml(line),
        );
        doc.y += h;
    }
    doc.rect(x0, doc.y, band, CODE_PAD, CODE_BG);
    doc.y += CODE_PAD;
}

/// Column widths: a column narrower than its fair share keeps its natural
/// width, the wide ones split what is left equally and wrap inside it.
fn column_widths(natural: &[f32], avail: f32) -> Vec<f32> {
    let n = natural.len();
    if natural.iter().sum::<f32>() <= avail {
        return natural.to_vec();
    }
    let mut width = vec![0.0; n];
    let mut fixed = vec![false; n];
    let mut left = avail;
    loop {
        let open: Vec<usize> = (0..n).filter(|&i| !fixed[i]).collect();
        if open.is_empty() {
            break;
        }
        let share = left / open.len() as f32;
        let fits: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&i| natural[i] <= share)
            .collect();
        if fits.is_empty() {
            for i in open {
                width[i] = share;
            }
            break;
        }
        for i in fits {
            width[i] = natural[i];
            fixed[i] = true;
            left -= natural[i];
        }
    }
    width
}

/// A table: header row on a tint, ruled cells, wrapped text, the header
/// repeated on every page the table runs onto.
fn table(doc: &mut Doc, t: &TableBuf, lists: usize) {
    let n = t
        .head
        .len()
        .max(t.rows.iter().map(Vec::len).max().unwrap_or(0));
    if n == 0 {
        return;
    }
    let x0 = indent(doc, lists);
    let avail = doc.right() - x0;
    let natural: Vec<f32> = (0..n)
        .map(|c| {
            let one_line = |cell: Option<&Vec<Run>>, bold: bool| {
                cell.map_or(0.0, |runs| {
                    wrap(runs, BODY, f32::INFINITY, bold)
                        .iter()
                        .map(line_width)
                        .fold(0.0, f32::max)
                })
            };
            let widest = t
                .rows
                .iter()
                .map(|r| one_line(r.get(c), false))
                .fold(one_line(t.head.get(c), true), f32::max);
            widest + CELL_PAD * 2.0 + 1.0
        })
        .collect();
    let widths = column_widths(&natural, avail);
    let h = BODY * LEADING;

    let laid = |cells: &[Vec<Run>], bold: bool| -> (Vec<Vec<Line>>, f32) {
        let lines: Vec<Vec<Line>> = (0..n)
            .map(|c| {
                cells.get(c).map_or_else(Vec::new, |runs| {
                    wrap(runs, BODY, widths[c] - CELL_PAD * 2.0, bold)
                })
            })
            .collect();
        let tallest = lines.iter().map(Vec::len).max().unwrap_or(1).max(1);
        (lines, tallest as f32 * h + CELL_PAD * 2.0)
    };
    let head = laid(&t.head, true);
    let draw = |doc: &mut Doc, (lines, row_h): &(Vec<Vec<Line>>, f32), fill: Option<&str>| {
        let mut x = x0;
        for (c, cell) in lines.iter().enumerate() {
            let w = widths[c];
            if let Some(fill) = fill {
                doc.rect(x, doc.y, w, *row_h, fill);
            }
            let _ = write!(
                doc.body,
                r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{row_h:.2}" fill="none" stroke="{LINE}" stroke-width="0.6"/>"#,
                y = doc.y,
            );
            for (i, line) in cell.iter().enumerate() {
                let room = w - CELL_PAD * 2.0 - line_width(line);
                let shift = match t.aligns.get(c) {
                    Some(Alignment::Center) => room / 2.0,
                    Some(Alignment::Right) => room,
                    _ => 0.0,
                };
                let baseline = doc.y + CELL_PAD + i as f32 * h + h * 0.5 + BODY * 0.35;
                doc.pieces(line, x + CELL_PAD + shift.max(0.0), baseline, INK);
            }
            x += w;
        }
        doc.y += row_h;
    };

    doc.room(head.1 + h + CELL_PAD * 2.0);
    draw(doc, &head, Some(HEAD_BG));
    for (r, row) in t.rows.iter().enumerate() {
        let body = laid(row, false);
        let page_before = doc.pages.len();
        doc.room(body.1);
        if doc.pages.len() != page_before {
            draw(doc, &head, Some(HEAD_BG));
        }
        draw(doc, &body, (r % 2 == 1).then_some(HEAD_BG));
    }
}

/// Every page of `src` rendered as a document, as SVG.
pub fn markdown_page_svgs(src: &str, opts: &PdfOptions) -> Vec<String> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut doc = Doc::new(opts);
    let mut runs: Vec<Run> = Vec::new();
    let mut style = Style::default();
    let mut block = Block::Paragraph;
    let mut code = String::new();
    let mut lists: Vec<List> = Vec::new();
    // An open list item that still owes its marker: its first paragraph
    // draws it, a second paragraph in the same item does not.
    let mut marker = false;
    let mut tbl: Option<TableBuf> = None;

    for event in Parser::new_ext(src, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => block = Block::Heading(level),
                Tag::BlockQuote(_) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    doc.quote += 1;
                }
                Tag::CodeBlock(_) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    block = Block::Code;
                    code.clear();
                }
                Tag::List(start) => {
                    // A tight parent item's text is still buffered when its
                    // nested list opens.
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    lists.push(List { next: start });
                }
                Tag::Item => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    marker = true;
                }
                Tag::Emphasis => style.italic = true,
                Tag::Strong => style.bold = true,
                Tag::Strikethrough => style.strike = true,
                Tag::Link { .. } => style.link = true,
                Tag::Table(aligns) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    tbl = Some(TableBuf {
                        aligns,
                        head: Vec::new(),
                        rows: Vec::new(),
                        row: Vec::new(),
                        cell: Vec::new(),
                    });
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    // List items get their gap from the item's end.
                    if lists.is_empty() {
                        doc.gap(GAP_PARAGRAPH);
                    }
                }
                TagEnd::Heading(_) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    block = Block::Paragraph;
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    doc.quote = doc.quote.saturating_sub(1);
                    doc.gap(GAP_BLOCK);
                }
                TagEnd::CodeBlock => {
                    code_block(&mut doc, &code, lists.len());
                    block = Block::Paragraph;
                    doc.gap(GAP_BLOCK);
                }
                TagEnd::List(_) => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    lists.pop();
                    if lists.is_empty() {
                        doc.gap(GAP_PARAGRAPH);
                    }
                }
                TagEnd::Item => {
                    flush(&mut doc, &mut runs, block, &lists, &mut marker);
                    marker = false;
                    if let Some(n) = lists.last_mut().and_then(|l| l.next.as_mut()) {
                        *n += 1;
                    }
                    doc.gap(GAP_ITEM);
                }
                TagEnd::Emphasis => style.italic = false,
                TagEnd::Strong => style.bold = false,
                TagEnd::Strikethrough => style.strike = false,
                TagEnd::Link => style.link = false,
                TagEnd::TableCell => {
                    if let Some(t) = tbl.as_mut() {
                        let cell = std::mem::take(&mut t.cell);
                        t.row.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(t) = tbl.as_mut() {
                        t.head = std::mem::take(&mut t.row);
                    }
                }
                TagEnd::TableRow => {
                    if let Some(t) = tbl.as_mut() {
                        let row = std::mem::take(&mut t.row);
                        t.rows.push(row);
                    }
                }
                TagEnd::Table => {
                    if let Some(t) = tbl.take() {
                        table(&mut doc, &t, lists.len());
                        doc.gap(GAP_BLOCK);
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(t) = tbl.as_mut() {
                    t.cell.push(Run {
                        text: text.into_string(),
                        style,
                    });
                } else if block == Block::Code {
                    code.push_str(&text);
                } else {
                    runs.push(Run {
                        text: text.into_string(),
                        style,
                    });
                }
            }
            Event::Code(text) => {
                let run = Run {
                    text: text.into_string(),
                    style: Style {
                        code: true,
                        ..style
                    },
                };
                match tbl.as_mut() {
                    Some(t) => t.cell.push(run),
                    None => runs.push(run),
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                let text = if matches!(event, Event::HardBreak) {
                    "\n"
                } else {
                    " "
                };
                let run = Run {
                    text: text.to_string(),
                    style,
                };
                match tbl.as_mut() {
                    Some(t) => t.cell.push(run),
                    None => runs.push(run),
                }
            }
            Event::Rule => {
                flush(&mut doc, &mut runs, block, &lists, &mut marker);
                doc.room(12.0);
                doc.y += 6.0;
                let (l, r) = (doc.left(), doc.right());
                doc.hline(l, r, doc.y, 1.0);
                doc.y += 6.0 + GAP_BLOCK;
            }
            _ => {}
        }
    }
    flush(&mut doc, &mut runs, block, &lists, &mut marker);
    doc.finish(&opts.title)
}

/// How many pages the document prints on, for the export dialog.
pub fn markdown_page_count(src: &str, opts: &PdfOptions) -> usize {
    markdown_page_svgs(src, opts).len()
}

/// The document as a PDF.
pub fn markdown_to_pdf(src: &str, opts: &PdfOptions) -> Result<Vec<u8>, String> {
    pages_to_pdf(&markdown_page_svgs(src, opts), opts.page_points())
}

#[cfg(test)]
#[path = "pdf_markdown_tests.rs"]
mod tests;
