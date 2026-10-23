use super::*;

fn run(text: &str) -> Run {
    Run {
        text: text.to_string(),
        style: Style::default(),
    }
}

fn pages(src: &str) -> Vec<String> {
    markdown_page_svgs(
        src,
        &PdfOptions {
            title: "notes.md".to_string(),
            ..Default::default()
        },
    )
}

#[test]
fn wrapped_lines_stay_inside_the_width() {
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                tempor incididunt ut labore et dolore magna aliqua. ";
    let lines = wrap(&[run(&text.repeat(4))], BODY, 200.0, false);
    assert!(lines.len() > 4, "{} lines", lines.len());
    for line in &lines {
        assert!(line_width(line) <= 200.0, "{:?}", line);
    }
    // Words are only broken when they have to be: every line ends on one.
    let joined: Vec<String> = lines
        .iter()
        .map(|l| {
            l.iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(joined.join(" "), text.repeat(4).trim_end());
}

#[test]
fn a_word_wider_than_the_line_is_cut_and_a_hard_break_breaks() {
    let lines = wrap(&[run(&"x".repeat(200))], BODY, 100.0, false);
    assert!(lines.len() > 1);
    assert!(lines.iter().all(|l| line_width(l) <= 100.0));
    let lines = wrap(&[run("one"), run("\n"), run("two")], BODY, 500.0, false);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1][0].text, "two");
}

#[test]
fn words_in_one_style_share_a_piece_and_styles_stay_apart() {
    let bold = Run {
        text: "bold".to_string(),
        style: Style {
            bold: true,
            ..Style::default()
        },
    };
    let lines = wrap(
        &[run("plain words "), bold, run(" again")],
        BODY,
        500.0,
        false,
    );
    let texts: Vec<&str> = lines[0].iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts, ["plain words", "bold", "again"]);
    assert_eq!(lines[0][1].style.face(), Face::Bold);
    // The space before "again" is a gap between pieces, not lost.
    assert!(lines[0][2].x > lines[0][1].x + lines[0][1].w);
}

#[test]
fn narrow_columns_keep_their_width_and_wide_ones_share_the_rest() {
    assert_eq!(column_widths(&[50.0, 60.0], 400.0), vec![50.0, 60.0]);
    let w = column_widths(&[40.0, 900.0, 700.0], 400.0);
    assert_eq!(w[0], 40.0);
    assert_eq!(w[1], 180.0);
    assert_eq!(w[2], 180.0);
}

#[test]
fn the_document_prints_rendered_not_as_source() {
    let svg = pages(
        "# Title\n\nSome **bold** and *slanted* text with `code`.\n\n\
         - first\n- second\n\n1. one\n2. two\n\n> quoted\n\n\
         ```\nlet x = 1;\n```\n\n| Name | Qty |\n|---|--:|\n| apple | 3 |\n",
    )
    .join("");
    // No Markdown syntax reaches the page.
    for syntax in ["# Title", "**", "`code`", "| Name", "```", "> quoted"] {
        assert!(!svg.contains(syntax), "{syntax} printed as source");
    }
    assert!(svg.contains(Face::Bold.svg_attrs()));
    assert!(svg.contains("skewX("), "italic is slanted");
    assert!(svg.contains(Face::Mono.svg_attrs()), "code is monospace");
    assert!(svg.contains("<circle"), "bullets are drawn");
    assert!(svg.contains(">1.</text>") && svg.contains(">2.</text>"));
    assert!(svg.contains(">let x = 1;</text>"));
    assert!(svg.contains(">apple</text>") && svg.contains(">Qty</text>"));
    assert!(svg.contains("notes.md  -  page 1 of 1"));
}

#[test]
fn a_long_document_pages_down_and_a_long_table_repeats_its_header() {
    let mut src = String::from("| Id | Item |\n|---|---|\n");
    for i in 0..150 {
        src.push_str(&format!("| {i} | thing {i} |\n"));
    }
    let svgs = pages(&src);
    assert!(svgs.len() > 1);
    for (i, svg) in svgs.iter().enumerate() {
        assert!(
            svg.contains(">Item</text>"),
            "page {} lost the header",
            i + 1
        );
        assert!(svg.contains(&format!("page {} of {}", i + 1, svgs.len())));
    }
    assert_eq!(
        markdown_page_count(&src, &PdfOptions::default()),
        svgs.len()
    );
}

#[test]
fn an_empty_document_is_one_blank_page() {
    assert_eq!(pages("").len(), 1);
}

#[test]
fn the_pdf_is_a_pdf() {
    let pdf = markdown_to_pdf("# Hi\n\nThere.\n", &PdfOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
