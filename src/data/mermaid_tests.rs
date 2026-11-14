use super::*;

/// One of each diagram type the documentation names.
const DIAGRAMS: &[(&str, &str)] = &[
    (
        "flowchart",
        "flowchart TD\n  A[Open file] --> B{Large?}\n  B -- yes --> C[Read a page]\n  B -- no --> D[Read it whole]",
    ),
    (
        "sequence",
        "sequenceDiagram\n  participant U as User\n  participant O as Octa\n  U->>O: Run query\n  O-->>U: result",
    ),
    (
        "class",
        "classDiagram\n  class Reader {\n    +read(path) Table\n  }\n  class CsvReader\n  Reader <|-- CsvReader",
    ),
    (
        "state",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Loading: open\n  Loading --> Idle: done",
    ),
    (
        "gantt",
        "gantt\n  dateFormat YYYY-MM-DD\n  section Build\n  Features :a1, 2026-11-01, 10d\n  Tests :after a1, 5d",
    ),
    (
        "pie",
        "pie title Formats\n  \"CSV\" : 45\n  \"Parquet\" : 55",
    ),
    (
        "entity relationship",
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places",
    ),
];

#[test]
fn every_documented_diagram_type_is_drawn_in_both_themes() {
    for (name, source) in DIAGRAMS {
        for dark in [false, true] {
            let picture = render(source, dark).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert!(
                picture.width() > 60 && picture.height() > 60,
                "{name} came out {} x {}",
                picture.width(),
                picture.height()
            );
            // Something is drawn on it: not every pixel is the same.
            let first = picture.get_pixel(0, 0);
            assert!(picture.pixels().any(|p| p != first), "{name} is blank");
        }
    }
}

#[test]
fn the_dark_theme_draws_on_a_dark_background() {
    let source = DIAGRAMS[0].1;
    let brightness = |dark: bool| {
        let p = render(source, dark).unwrap();
        let [r, g, b, _] = p.get_pixel(0, 0).0;
        u32::from(r) + u32::from(g) + u32::from(b)
    };
    assert!(brightness(true) < brightness(false));
}

#[test]
fn text_that_is_no_diagram_is_an_error_not_an_empty_picture() {
    for source in ["", "flowchart TD\n  A[ --> ", "just some words"] {
        assert!(render(source, false).is_err(), "{source:?} drew something");
    }
}
