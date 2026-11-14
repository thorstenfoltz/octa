"""Build assets/countries-110m.json.gz from Natural Earth admin-0 countries
at 1:110m (public domain). Keeps the ISO codes and every name field, rounds
coordinates to 3 decimals (about 100 m, far below what 1:110m can show) and
gzips the result. Re-run only to update the outlines."""

import gzip
import json
import urllib.request
from pathlib import Path

URL = (
    "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/"
    "v5.1.2/geojson/ne_110m_admin_0_countries.geojson"
)
KEEP = {"ISO_A2", "ISO_A3", "ISO_A2_EH", "ISO_A3_EH", "ADM0_A3", "NAME", "NAME_LONG", "FORMAL_EN"}


def rounded(coords):
    if isinstance(coords[0], (int, float)):
        return [round(coords[0], 3), round(coords[1], 3)]
    return [rounded(c) for c in coords]


def main() -> None:
    with urllib.request.urlopen(URL, timeout=60) as resp:
        data = json.load(resp)
    for f in data["features"]:
        props = f["properties"]
        f["properties"] = {
            k: v
            for k, v in props.items()
            if (k in KEEP or (k.startswith("NAME_") and len(k) <= 8)) and v not in (None, "", "-99")
        }
        f["geometry"]["coordinates"] = rounded(f["geometry"]["coordinates"])
    out = Path(__file__).resolve().parent.parent / "assets" / "countries-110m.json.gz"
    raw = json.dumps(data, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    out.write_bytes(gzip.compress(raw, compresslevel=9, mtime=0))
    print(f"{out}: {len(data['features'])} countries, {out.stat().st_size} bytes")


if __name__ == "__main__":
    main()
