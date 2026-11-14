"""Build assets/NotoSansMath-subset.ttf and assets/NotoSansSymbols-subset.ttf:
the arrows, operators, technical signs, circled numbers and other symbols that
no other bundled font draws. Noto Sans Math goes first, Noto Sans Symbols
fills what is then still missing. Re-run only to change what is covered.

    uv run --no-project --with fonttools scripts/build-symbol-fonts.py [FONT_DIR]

FONT_DIR holds NotoSansMath-Regular.ttf (3.000) and NotoSansSymbols-Regular.ttf
(2.003), by default the folder the noto-fonts package installs them into."""

import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

ASSETS = Path(__file__).resolve().parent.parent / "assets"
# Letterlike; arrows, operators, technical; enclosed alphanumerics; misc
# symbols, dingbats, misc maths A, arrows A; arrows B, misc maths B, more
# operators, symbols and arrows.
RANGES = [(0x2100, 0x214F), (0x2190, 0x23FF), (0x2460, 0x24FF), (0x2600, 0x27FF), (0x2900, 0x2BFF)]
SOURCES = [
    ("NotoSansMath-Regular.ttf", "NotoSansMath-subset.ttf"),
    ("NotoSansSymbols-Regular.ttf", "NotoSansSymbols-subset.ttf"),
]


def main() -> None:
    font_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/noto")
    outputs = {out for _, out in SOURCES}
    have: set[int] = set()
    for path in sorted(ASSETS.glob("*.[ot]tf")):
        if path.name not in outputs:
            have |= set(TTFont(path).getBestCmap())
    wanted = {c for lo, hi in RANGES for c in range(lo, hi + 1)} - have

    for source, out in SOURCES:
        font = TTFont(font_dir / source)
        take = wanted & set(font.getBestCmap())
        wanted -= take
        options = subset.Options()
        # Every name record stays, so the file still states its licence. No
        # layout tables: egui draws one glyph per character.
        options.name_IDs = ["*"]
        options.layout_features = []
        options.drop_tables += ["MATH", "GSUB", "GPOS", "GDEF"]
        options.hinting = False
        options.notdef_outline = True
        subsetter = subset.Subsetter(options)
        subsetter.populate(unicodes=take)
        subsetter.subset(font)
        font.save(ASSETS / out)
        print(f"{out}: {len(take)} characters, {(ASSETS / out).stat().st_size:,} bytes")
    print(f"in no font: {len(wanted)}")


if __name__ == "__main__":
    main()
