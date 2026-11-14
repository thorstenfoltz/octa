# Number bands

**Columns > Number bands...** works both ways round:

- **Numbers to bands** turns a column of numbers into a column of bands, such
  as ages into `18-34`, `35-64` and `65+`, ready to count, chart or pivot.
- **Bands to numbers** turns a column of bands back into numbers, such as an
  age-group column from a survey back into approximate ages.

The new column goes right after the one you pick, and one Undo takes it away
again. The source column is never changed.

## Numbers to bands

| Field    | What it does                                                                                                                 |
|----------|------------------------------------------------------------------------------------------------------------------------------|
| Column   | The column of numbers. Text that reads as a number counts; other cells stay empty.                                           |
| Bands by | **Equal widths**, **Equal counts** or **Own edges**.                                                                         |
| How many | For equal widths and equal counts: how many bands, 1 to 1000.                                                                |
| Edges    | For own edges: where each band starts, such as `18, 35, 65`. Use semicolons when the numbers have decimal commas (`1,5; 3`). |

- **Equal widths** cuts from the smallest to the largest number into bands of
  the same width. Whole numbers get a whole width, so 0 to 99 in five bands is
  `0-19`, `20-39`, ... `80-99`.
- **Equal counts** puts about as many values into each band: 4 gives
  quartiles, 10 deciles. Equal values cannot be split, so a column where most
  values are the same gives fewer bands.
- **Own edges** gives a band below the first edge, one between each two, and
  one from the last edge on: `18, 35, 65` makes `<18`, `18-34`, `35-64` and
  `65+`.

A band holds its lower edge and not the next one, except the last band of
equal widths or counts, which ends at the largest value. In a column of whole
numbers the labels name both ends (`18-34` holds 34). Other numbers get
interval labels: `[1.5, 3)` holds 1.5 but not 3, `[3, 4.5]` both.

## Bands to numbers

| Field          | What it does                                                         |
|----------------|----------------------------------------------------------------------|
| Column         | The column of band labels.                                           |
| Number to give | **The middle** of each band, **the lower end** or **the upper end**. |

Labels are read the way people write them, so bands from anywhere work:

| Label                                                 | Read as                                                                                                                                                |
|-------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
| `18-24`, `18 - 24`, `18 to 24`, `18 bis 24`           | 18 to 24                                                                                                                                               |
| `[18, 25)`                                            | 18 to 25                                                                                                                                               |
| `10,000-19,999`, `€20k-30k`                           | 10000 to 19999, 20000 to 30000                                                                                                                         |
| `65+`, `>65`                                          | 65 and up                                                                                                                                              |
| `<18`                                                 | up to 18                                                                                                                                               |
| `65 and over`, `65 und älter`, `under 18`, `unter 18` | a lone number with words sits at the end of the other bands it is next to: above the highest band it is the open top, below the lowest the open bottom |
| `0`, `100`                                            | that number                                                                                                                                            |

**An open band takes the width of the band next to it**, so `65+` beside
`55-64` reads as 65 to 74, and its middle is 69.5. A label with no number in
it (`unknown`) stays empty, and the status line says how many there were.
`18-24` gives 21 as its middle; whole-number bands are counted from end to
end, so `55-64` gives 59.5.

## On a database tab or a partly loaded file

The bands are made from the rows the tab holds, and the status line says so
when that is not the whole table.

## Command line and MCP

`octa --bands` (see [`--bands`](../cli/bands.md)) and the
[`number_bands`](../mcp/tools/number_bands.md) MCP tool do the same.
