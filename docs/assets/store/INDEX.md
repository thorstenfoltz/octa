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

- Screenshot - the listing reuses the documentation hero shot at
  `docs/assets/screenshots/hero-table-view.png` (3450x2028, well over the
  1366x768 Store minimum). One shared English screenshot serves all 32
  listings via the CSV's `default` column. To use a different one, drop a PNG
  into the build's `<outdir>/shots/` and point `_default.toml` at it.

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
  translated from the English. `en-us` has no file: it inherits `_default`.
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
