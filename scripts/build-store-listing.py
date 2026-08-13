#!/usr/bin/env python3
"""Fill a Partner Center listing CSV from per-language TOML files.

    ./scripts/build-store-listing.py EXPORTED.csv OUTDIR

Partner Center exports one CSV with a `default` column plus one column per
language it detected in the uploaded packages. Anything left blank in a
language column falls back to `default`, so English content only has to be
written once.

This script:
  * writes `default` from docs/assets/store/listings/_default.toml
  * writes each language column from docs/assets/store/listings/<code>.toml
    (missing file = that language inherits `default`, which is fine)
  * drops rows for surfaces Octa does not ship on (Xbox, Holographic,
    SurfaceHub, mobile) to keep the file reviewable. Partner Center treats a
    deleted row as "leave this field alone".
  * leaves Field / ID / Type untouched, which Partner Center requires.

Trailer rows are NEVER dropped: for those, deleting the row deletes the asset
from Partner Center itself.

Output goes to OUTDIR alongside the screenshots, because image fields are
folder-relative paths and Partner Center wants the whole folder imported.
"""

from __future__ import annotations

import csv
import re
import shutil
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LISTINGS = REPO / "docs" / "assets" / "store" / "listings"

# The import folder's own name has to prefix every asset path.
FOLDER_NAME = "octa-listing"

# Field families to drop. Octa is desktop-only, so these surfaces would just
# be hundreds of empty rows to scroll past.
DROP = re.compile(
    r"^(Mobile|Xbox|Holographic|SurfaceHub)(Screenshot|ScreenshotCaption)\d+$"
    r"|^XboxBrandedKeyArt|^XboxTitledHero|^XboxFeaturedPromo"
    r"|^OptionalPromo|^PromoImage"
)
# Never drop these, whatever else matches.
KEEP = re.compile(r"^Trailer")


def load(name: str) -> dict:
    # BCP-47 tags are case-insensitive and Partner Center is not consistent
    # about them (sr-Cyrl vs sr-cyrl, zh-Hans vs zh-hans). Filenames are
    # lowercase, so match on that rather than on whatever the export used.
    path = LISTINGS / f"{name.lower()}.toml"
    if not path.exists():
        # Partner Center exports the bare tag for a language that has only one
        # script in the package (`sr`, `zh`), while the content files are named
        # after the script (`sr-cyrl`, `zh-hans`). Fall back to the single
        # script variant so the column is not silently left English.
        variants = sorted(LISTINGS.glob(f"{name.lower()}-*.toml"))
        if len(variants) != 1:
            return {}
        path = variants[0]
    with path.open("rb") as fh:
        return tomllib.load(fh)


def cell_for(field: str, content: dict) -> str | None:
    """Value for one Field row, or None to leave the cell as it is."""
    if field == "Description":
        return content.get("long")
    if field == "ShortDescription":
        return content.get("short")
    m = re.fullmatch(r"Feature(\d+)", field)
    if m:
        features = content.get("features", [])
        idx = int(m.group(1)) - 1
        return features[idx] if idx < len(features) else None
    # Remaining fields are English-only and live in _default.toml under their
    # exact Partner Center field name.
    return content.get("fields", {}).get(field)


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    src, outdir = Path(sys.argv[1]).expanduser(), Path(sys.argv[2]).expanduser()

    with src.open(newline="", encoding="utf-8-sig") as fh:
        rows = list(csv.reader(fh))
    header, body = rows[0], rows[1:]

    languages = header[4:]
    default = load("_default")
    if not default:
        print("error: docs/assets/store/listings/_default.toml is missing")
        return 1
    per_lang = {code: load(code) for code in languages}

    kept, dropped = [], 0
    for row in body:
        field = row[0]
        if field and DROP.match(field) and not KEEP.match(field):
            dropped += 1
            continue
        if field:
            managed = cell_for(field, default)
            if managed is not None:
                row[3] = managed
            for i, code in enumerate(languages, start=4):
                value = cell_for(field, per_lang.get(code) or {})
                if value is not None:
                    row[i] = value
                elif managed is not None:
                    # A field this repo owns and this language does not
                    # translate must inherit `default`, which only a BLANK cell
                    # does. Partner Center exports some fields filled in per
                    # language (OverrideLogosForWin10 comes back as False in
                    # all 32), and a stale value there silently beats default.
                    row[i] = ""
        kept.append(row)

    target = outdir / FOLDER_NAME
    target.mkdir(parents=True, exist_ok=True)
    out_csv = target / src.name
    # Partner Center requires UTF-8; a BOM keeps Excel from mangling the
    # non-Latin columns if the file gets opened there.
    with out_csv.open("w", newline="", encoding="utf-8-sig") as fh:
        csv.writer(fh).writerows([header] + kept)

    # Partner Center resolves asset paths relative to the imported folder, so
    # every referenced image has to be copied in. Look each one up by the name
    # the CSV actually uses, searching fresh captures first and then the
    # repo's tracked assets, so committed artwork needs no manual copying.
    shots = outdir / "shots"
    search = [shots, REPO / "docs/assets/store", REPO / "docs/assets/screenshots"]
    referenced = {
        v.split("/", 1)[1]
        for row in kept
        for v in row[3:]
        if v.startswith(f"{FOLDER_NAME}/")
    }
    copied, found_in = 0, {}
    for name in sorted(referenced):
        for folder in search:
            candidate = folder / name
            if candidate.is_file():
                shutil.copy2(candidate, target / name)
                copied += 1
                found_in[name] = folder
                break

    translated = sorted(c for c in languages if per_lang.get(c))
    print(f"wrote {out_csv}")
    print(f"  rows kept {len(kept)}, dropped {dropped}")
    print(f"  languages with their own text: {len(translated)}/{len(languages)}")
    if translated:
        print(f"    {' '.join(translated)}")
    missing = [c for c in languages if not per_lang.get(c)]
    if missing:
        print(f"  inheriting `default`: {' '.join(missing)}")
    print(f"  images copied in: {copied}/{len(referenced)}")
    for name in sorted(found_in):
        print(f"    {name}  <- {found_in[name]}")
    # Name what is still missing: an import short of one asset is rejected
    # outright, and the portal error does not say which file.
    absent = sorted(n for n in referenced if n not in found_in)
    if absent:
        print(f"  MISSING, import will be rejected: {' '.join(absent)}")
        print(f"  put them in {shots}/ and re-run")
    print(f"\nImport {target} with 'Import listings' -> 'Import folder'.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
