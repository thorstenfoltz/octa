# Microsoft Store listing assets

Everything the Partner Center listing needs. `scripts/build-store-listing.py`
assembles these into an import folder; nothing here is copied by hand.

## Images

- `store-tile-300x300.png` - the 300x300 Store logo. Rendered from
  `assets/octa-rose.svg` (not from `assets/octa-rose.png`, which is 256px and
  would have to be upscaled):

      magick -background none -density 1200 assets/octa-rose.svg \
             -resize 300x300 docs/assets/store/store-tile-300x300.png

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
  `_default.toml` sets that flag alongside the two paths. The flag comes back
  from an export filled in as `False` in every language column, and a filled
  cell beats `default`, which is why the build script blanks language cells for
  fields it owns.

- Screenshots - six, all reused from the documentation captures in
  `docs/assets/screenshots/`: `hero-table-view.png`, `sql-view.png`,
  `chatbot.png`, `db-sidebar-tree.png`, `json-tree-view.png` and
  `settings-dialog.png`. Every one clears the 1366x768 Store minimum in both
  dimensions; check that before adding another. They are English-only fields,
  so one set serves all 32 listings via the CSV's `default` column. To use a
  different capture, drop a PNG into the build's `<outdir>/shots/` and point
  `_default.toml` at it.

`build-store-listing.py` resolves each referenced filename by searching
`<outdir>/shots/`, then `docs/assets/store/`, then
`docs/assets/screenshots/`, so committed artwork is picked up automatically
and a freshly captured file in `shots/` overrides it.

## Text

- `listings/_default.toml` - English `short` and `long`, plus the non-prose
  fields (search terms, developer name, asset paths). **This is the only
  source of the English copy**; the `default` column is what every language
  column falls back to.
- `listings/<code>.toml` - 31 languages, each written natively rather than
  translated from the English, and addressing the reader informally the way
  `locales/` does. `en-us` has no file: it inherits `_default`. Serbian is
  `sr-cyrl.toml` while the export column is called `sr`; the script falls back
  to the single script variant of a bare tag, so both spellings resolve.
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
