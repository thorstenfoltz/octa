# Microsoft Store listing assets

Everything the Partner Center listing needs. `scripts/build-store-listing.py`
assembles these into an import folder; nothing here is copied by hand.

## Images

- `store-tile-300x300.png` - the 300x300 Store logo. Rendered from
  `assets/octa-rose.svg` (not from `assets/octa-rose.png`, which is 256px and
  would have to be upscaled):

      magick -background none -density 1200 assets/octa-rose.svg \
             -resize 300x300 -depth 8 \
             PNG32:docs/assets/store/store-tile-300x300.png

  `-depth 8 PNG32:` matters: without it magick writes a 16-bit PNG, which the
  other two tiles (`rsvg-convert`) are not, and no Store tool asks for 16 bits.

  Transparent background, matching the source and the in-package logos. For a
  solid one, swap `-background none` for e.g. `-background white` and add
  `-flatten`.

- `store-tile-150x150.png` and `store-tile-71x71.png` - the two smaller Store
  logos, in emerald and blue rather than the rose of the 300x300 one:

      rsvg-convert -w 150 -h 150 assets/octa-emerald.svg \
                   -o docs/assets/store/store-tile-150x150.png
      rsvg-convert -w 71 -h 71 assets/octa-blue.svg \
                   -o docs/assets/store/store-tile-71x71.png

  Partner Center only uses them when `OverrideLogosForWin10` is `True`, so
  `_default.toml` sets that flag alongside the two paths, and the script writes
  both the flag and the paths into every language column (see below).

- Screenshots - six, all reused from the documentation captures in
  `docs/assets/screenshots/`: `hero-table-view.png`, `sql-view.png`,
  `chatbot.png`, `db-sidebar-tree.png`, `json-tree-view.png` and
  `settings-dialog.png`. Every one clears the 1366x768 Store minimum in both
  dimensions; check that before adding another. There is one English set and
  no per-language captures. To use a different one, drop a PNG into the build's
  `<outdir>/shots/` and point `_default.toml` at it.

  **Asset cells are not inherited, unlike text.** Partner Center holds an image
  per language listing: export the listing and every language column carries
  its own dashboard URL, not just `default`. A blank language cell there means
  *that listing has no picture*, not *use the English one*, and the same goes
  for `OverrideLogosForWin10`. So the script writes the relative path into all
  34 columns, the one English file over and over. Only text fields
  (`DevStudio`, `CopyrightTrademarkInformation`, and any `short` / `long` /
  `features` a language does not translate) stay blank to inherit `default`.

`build-store-listing.py` resolves each referenced filename by searching
`<outdir>/shots/`, then `docs/assets/store/`, then
`docs/assets/screenshots/`, so committed artwork is picked up automatically
and a freshly captured file in `shots/` overrides it.

## Text

- `listings/_default.toml` - English `short` and `long`, plus the non-prose
  fields (search terms, developer name, asset paths). **This is the only
  source of the English copy**; the `default` column is what every language
  column falls back to.
- `listings/<code>.toml` - 31 languages, each **written** in that language
  rather than translated from the English, in a **neutral, serious register
  with no direct address** (unlike `locales/`, which is informal): impersonal
  phrasing, full sentences in the bullets, unambiguous terms ("API key", not
  "key"). Written means written: its own sentence rhythm, the words that
  language's own app listings use. The 2026-09-05 pass produced a faithful
  rendering of the English; a colloquial rewrite on 2026-09-10 ("double-click
  and there is the table", "with undo") was rejected the same day and replaced
  by the neutral version. A new language starts from the facts in
  `content-brief.txt` (formats, SQL, local processing, 32 languages, MIT), not
  from the English sentences. The bullet **count** must stay the same as `_default`,
  though: a missing `Feature<N>` leaves the cell blank, and a blank cell
  inherits the English one. `en-us` has no file: it inherits `_default`.
  Serbian is `sr-cyrl.toml` while the export column is called `sr`; the script
  falls back to the single script variant of a bare tag, so both spellings
  resolve.
- `SearchTerm1..7` in every listing file - the Store keywords. Max 7 terms,
  30 characters each, 21 words in total, and **none of them may name a product
  published by somebody else**. `excel alternative` failed the whole 0.19.1
  submission in September 2026, flagged in all 32 languages at once; the
  replacement is `xlsx viewer` in each language.
- **The word Excel appears nowhere in a listing**, keywords or prose. The
  spreadsheet format is called `XLSX` in all 33 files. It read as harmless
  in a list of formats, but the rejection is per submission, not per field,
  and the review cycle is measured in days. `build-store-listing.py` fails
  with exit 1 rather than write a CSV that mentions it, in any script.
- `content-brief.txt` - the facts every listing must convey. Not a text to
  translate; it exists so 32 independently written texts stay factually
  consistent when a feature changes.
- `runfulltrust-justification.txt` - answer for the restricted-capability
  prompt under **Eigenschaften** / Properties. 461 characters, fits the field
  limit. Paste as-is.

## Supported languages

Not a form field. They come from the `<Resource Language="..."/>` list in
`windows/AppxManifest.xml` (all 32), which is what Partner Center reads off
the uploaded package to decide which listing columns exist. Keep that list in
sync with `locales/` and `docs/reference/languages.md`.

## Building the import folder

    ./scripts/build-store-listing.py ~/Downloads/listingData-*.csv ~/Downloads/octa-store

The first argument is the CSV exported from Partner Center (**export it after
uploading a package**, or it will have no language columns). The result is an
`octa-listing` folder holding the filled CSV plus every referenced image;
import it with **Import listings -> Import folder**.
