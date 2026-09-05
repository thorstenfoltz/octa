"""Source files open in the Raw view with syntax colouring.

Octa's text reader claims a long list of source and config extensions, so a
.py, .rs, .go, .ts or .tf file lands here rather than being refused.
"""

import csv
from pathlib import Path

PRODUCTS = Path(__file__).parent.parent / "tables" / "products.csv"


def average_price(rows: list[dict[str, str]]) -> float:
    """Mean price across every row, to the cent."""
    return round(sum(float(r["price"]) for r in rows) / len(rows), 2)


def main() -> None:
    with PRODUCTS.open(encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    print(f"{len(rows)} products, average {average_price(rows):.2f}")
    for row in rows:
        if not row["note"]:
            continue
        print(f"  {row['product']:<20} {row['note']}")


if __name__ == "__main__":
    main()
