"""Rebuild assets/cldr-names.tsv.gz: currency and country names in every
language Octa speaks, from Unicode CLDR through Babel.

    source .venv/bin/activate && python scripts/build-names.py

One line per name: language, C (currency) or T (territory), code, name.
Read by src/data/reference/names.rs.
"""

import gzip
import pathlib

from babel import Locale

ROOT = pathlib.Path(__file__).resolve().parent.parent
BABEL = {"pt-br": "pt_BR", "pt-pt": "pt_PT", "sr-cyrl": "sr_Cyrl", "zh-hans": "zh_Hans", "zh-hant": "zh_Hant"}

lines = []
for path in sorted((ROOT / "locales").glob("*.toml")):
    lang = path.stem
    loc = Locale.parse(BABEL.get(lang, lang))
    for code, name in sorted(loc.currencies.items()):
        if len(code) == 3 and code.isalpha():
            lines.append(f"{lang}\tC\t{code}\t{name}")
    for code, name in sorted(loc.territories.items()):
        if len(code) == 2 and code.isalpha():
            lines.append(f"{lang}\tT\t{code}\t{name}")

out = ROOT / "assets" / "cldr-names.tsv.gz"
# mtime=0 so an unchanged rebuild is byte-identical.
with gzip.GzipFile(out, "wb", mtime=0) as f:
    f.write(("\n".join(lines) + "\n").encode())
print(f"{len(lines)} names -> {out} ({out.stat().st_size} bytes)")
