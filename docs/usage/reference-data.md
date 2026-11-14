# Reference data

**Data > Reference data...** adds columns that need public data from outside
your file. One dialog, three modes:

- **Convert currency** converts an amount at the exchange rate of the row's
  own date, so an invoice from March 2019 is converted at the rate of that
  day, not today's.
- **In today's money** expresses an amount from one month in the prices of
  another, by the consumer price index of its country. 100 euros from 2015
  become what they are worth now.
- **Places and region keys** looks up a postcode, a municipality code, a
  Kreisschlüssel or a NUTS code and adds the place, its coordinates and the
  other keys. The postcode `47877` in Germany gives the municipality
  `05166036` Willich, the Kreis `05166` Kreis Viersen and the NUTS 3 region
  `DEA1E`.

The new columns go right after the column they come from, and one Undo takes
them away again. The source column is never changed. A status line says how
many rows were found.

## Before you start

**Everything here is off until you switch it on.** Octa ships none of this
data. Open **Settings > Online data** and switch on the downloads you want;
Octa then fetches the files, keeps them on your computer and refreshes them at
the interval you set. Nothing about your data is sent, see
[Privacy](../privacy.md).

| Mode                   | Needs                                                                          |
|------------------------|--------------------------------------------------------------------------------|
| Convert currency       | **Exchange rates**                                                             |
| In today's money       | **Inflation**                                                                  |
| Places and region keys | **Postcodes**, **European regions** or **German postcodes and municipalities** |

The dialog always opens. A mode whose download is off is greyed, says which
switch it needs and offers **Open Settings**. In Places and region keys each
output is greyed on its own, so one switched-on source is enough to start.

## Convert currency

| Field                  | What it does                                                                                            |
|------------------------|---------------------------------------------------------------------------------------------------------|
| Amount                 | The column with the amounts. Text holding numbers works too, European ones such as `1.234,56` included. |
| Currency of the amount | **From a column** of codes (`EUR`) or names (`Euro`), or **The same for every row**.                    |
| Convert to             | The currency of the new column.                                                                         |
| Date                   | The column whose date picks the rate, or **Latest rate** for the newest rate on every row.              |
| Rounding               | **Usual decimals of the currency**, **Round to** a number of decimals, or **Do not round**.             |
| Currency shown as      | **Official code** or **Symbol**, for the currency column.                                               |

Three columns are added: the converted amount `<amount>_<TO>`, the currency
`<amount>_<TO>_currency` and `<amount>_<TO>_rate_date`, which names the rate
that was used. The amount stays a plain number, so it still sums and sorts.

Which rate a row gets, in this order:

1. The daily rate of the row's date.
2. On a weekend or holiday, the last daily rate before it.
3. For a currency without daily rates, the average of that month. The rate
   date column then reads `2024-03 (monthly average)`.
4. For a date after the newest rate, the newest rate. The rate date column
   shows its date.
5. For a date before the first rate, an unknown currency or an empty amount,
   the cell stays empty and the rate date column says why.

The Bank for International Settlements quotes every currency against the US
dollar, so a conversion between two other currencies goes through the dollar:
the target's rate divided by the source's rate of the same date. The
arithmetic is exact decimal arithmetic, never floating point. When both
currencies are the same the amount is copied unchanged.

**Rounding.** The usual decimals are 2 for most currencies, 0 for the yen, the
won and the other currencies without a minor unit, and 3 for the dinars
(Bahrain, Iraq, Jordan, Kuwait, Libya, Tunisia) and the Omani rial. **Do not
round** keeps every digit the calculation gives, up to 20 decimals.

**Currency shown as.** The official code (`EUR`, `USD`, `JPY`) is the default
and clear in every language. **Symbol** writes `€` or `£` instead, but only
where the symbol belongs to one currency alone: € £ ₹ ₪ ₩ ₦ ₽ ₺ ₴ ₫ ₸ ₾ ₼ ₲ ₡ ₵
₭ ₮ and zł. `$` is shared by the US, Canadian and Australian dollar and many
more, `¥` by the yen and the yuan, and `kr`, `Fr.` and `₱` are shared too, so
those currencies keep their code.

A made-up example (these are not real rates): 100 EUR dated Saturday
2026-01-03, converted to USD, takes the rate of Friday 2026-01-02 and gives

| amount | amount_USD | amount_USD_currency | amount_USD_rate_date |
|--------|------------|---------------------|----------------------|
| 100    | 105.26     | USD                 | 2026-01-02           |

## In today's money

| Field            | What it does                                                                         |
|------------------|--------------------------------------------------------------------------------------|
| Amount           | The column with the amounts.                                                         |
| Date             | **From a column** of dates, or **The same for every row**: one year and month.       |
| Country          | **From a column** of names or codes, or one country. The euro area counts as one.    |
| In the prices of | The month the amounts are expressed in. Default: the newest month every country has. |
| Rounding         | **Round to** a number of decimals (default 2) or **Do not round**.                   |

The new amount is the old one, times the price index of the target month,
divided by the price index of the row's month. An amount of 100 from a month
with index 100, in the prices of a month with index 132, is 132.

Two columns are added: `<amount>_in_<month>` and `<amount>_index_month`, which
names the two months used (`2015-01 to 2026-01`). A country whose index ends
before the target month uses its newest month, and the index month column says
so. A date before the first month, or a country that is not covered, leaves
the cell empty with the reason.

The lists show currencies and countries by name, in the language Octa is set
to, with the code after it: `Euro (EUR)`, `Germany (DE)`.

Countries are recognised by name in many languages and by ISO code. The index
is each country's own consumer price index, monthly, so the result is that
country's inflation, in that country's currency.

## Places and region keys

| Field            | What it does                                                                              |
|------------------|-------------------------------------------------------------------------------------------|
| Key column       | The column holding the postcodes or codes to look up.                                     |
| The column holds | **Postcodes**, **Municipality codes**, **Kreisschlüssel** or **NUTS codes**.              |
| Country          | **From a column**, or one country. Kreisschlüssel and NUTS codes carry their own country. |
| Add              | The columns to add, see below.                                                            |
| Several matches  | **All, joined** or **First only**.                                                        |

| Output                                        | Comes from                                                                |
|-----------------------------------------------|---------------------------------------------------------------------------|
| Place, Region, Latitude, Longitude            | Postcodes (GeoNames)                                                      |
| Postcode (when you look up a code)            | German postcodes and municipalities (OpenPLZ), European regions elsewhere |
| Municipality code and name                    | OpenPLZ for Germany (exact), European regions elsewhere                   |
| Kreisschlüssel and Kreis name                 | The first five digits of the municipality code; the name from GeoNames    |
| NUTS 1, NUTS 2 and NUTS 3, each code and name | European regions (Eurostat)                                               |

Each output becomes a column named `<key>_<output>`, such as `plz_place` or
`plz_nuts3`.

**All** and **None** tick or untick every column at once.

**Which key works where.** Postcodes work in every country listed under
Coverage. Municipality codes work in the 34 European countries Eurostat
covers; in Germany that code is the Gemeindeschlüssel. The Kreisschlüssel is
German only, and the dialog lists it only once German municipality codes are
stored (**European regions** or **German postcodes and municipalities**
downloaded). Other countries have no such key: use the NUTS 3 code there,
which in Germany names the same area as the Kreis.

**Several matches, both ways.** A postcode can lie in several places and
municipalities, and a municipality, Kreis or region holds many postcodes. In
Germany 1,757 postcodes lie in more than one municipality (`01945` covers 11),
and the Kreis `05166` holds 12 postcodes.

- **All, joined** (default) puts every match in one cell, sorted and separated
  by commas: `Guteborn, Ruhland, Tettau`. Coordinates are the centre of the
  matched places.
- **First only** gives the first match in alphabetical order, with its own
  coordinates.

OpenPLZ lists a few streets of `47877` under Mönchengladbach (`05116000`), so
with **All, joined** the municipality cell for that postcode holds both codes.

For a municipality code, Kreisschlüssel or NUTS code, **Place** lists the
places of the postcodes inside it and the coordinates are their centre.

**Octa tidies the keys before matching.** A postcode column that was read as
numbers gets its leading zeros back (`1945` becomes `01945`) wherever the
country's postcodes all have one length. Spaces and upper or lower case do not
matter, so `sw1a1aa` matches `SW1A 1AA`. Municipality codes and Kreisschlüssel
are padded the same way.

Coordinates mark the area of a postcode, not a street or a house.

## Coverage and known limits

- **Exchange rates**: 147 currencies. 61 have daily rates, most of them since
  about 1970; the others have monthly averages.
- **Inflation**: the monthly consumer price index of 63 economies and the euro
  area.
- **Postcodes**: the 124 countries and territories GeoNames publishes, with
  full postcodes for the United Kingdom, the Netherlands and Canada. Chile,
  China, Ireland and Malta have only the first part of the postcode, Argentina
  the first five characters, Brazil only the main codes.
- **European regions**: 34 countries, the EU 27 plus Switzerland, Iceland,
  Liechtenstein, Norway, North Macedonia, Serbia and Turkey. Greece is
  covered here although GeoNames lacks it.
- Outside Germany, Eurostat places each postcode at one point and names the
  municipality that point lies in. A postcode that crosses a municipal border
  therefore names only one municipality. For Germany, OpenPLZ gives every
  pair.
- Iceland has no sheet in Eurostat's municipality workbook, so its NUTS 3
  regions come from its postcodes.

## Where the data lives

In the `reference` folder next to Octa's settings file, as Parquet files:

| File                               | Holds                                        | About           |
|------------------------------------|----------------------------------------------|-----------------|
| `currency_daily.parquet`           | One row per day, one column per currency     | 1.6 MB          |
| `currency_monthly.parquet`         | One row per month, one column per currency   | 0.4 MB          |
| `inflation.parquet`                | One row per month, one column per economy    | 0.1 MB          |
| `postcodes/<country>.parquet`      | Postcode, place, region, coordinates         | 0.25 MB to 7 MB |
| `eu_postcodes.parquet`             | Country, postcode, NUTS 3, municipality code | 1.4 MB          |
| `eu_municipalities.parquet`        | Country, municipality code, name, NUTS 3     | 1 MB            |
| `nuts.parquet`                     | NUTS code and name                           | 40 KB           |
| `de_postcode_municipality.parquet` | German postcode and municipality pairs       | 30 KB           |

They are ordinary Parquet files, so Octa can open them like any other table.
Rates and indices are stored as exact decimals.

Switching a download off keeps its data, and the dialog keeps working with
it. **Delete downloaded data** in **Settings > Online data** removes it.

## How refresh works

Each download has its own **Refresh every** interval, in days or months, one
month by default. When the interval has passed, Octa looks again at start-up
and before the dialog applies. Postcodes are fetched per country, the first
time a country is used.

- **Unchanged files are skipped.** Octa asks the server whether the file has
  changed and downloads it only if it has.
- **A new download never loses what you have.** A new value replaces a stored
  one only where the new download has a value; anything it lacks stays as it
  was. For postcodes the unit is one postcode: when the new download lists it,
  what it says replaces the old rows for that postcode.
- **A series on a new base is replaced whole.** If a price index moves from
  2010 = 100 to 2020 = 100, or a currency is redenominated, old and new values
  are on different scales and must not be mixed. Octa notices that every
  overlapping value differs by the same factor and takes that series entirely
  from the new download.
- **A failed download changes nothing.** A window names the source and the
  reason, with text you can select and copy, and the data downloaded before
  stays in use. See
  [Troubleshooting](../troubleshooting.md#a-reference-download-fails).

## Sources and licences

| Download                            | Credit                                                                                                                               |
|-------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| Exchange rates, Inflation           | Source: BIS (Bank for International Settlements), [data.bis.org](https://data.bis.org/help/legal)                                    |
| Postcodes                           | [GeoNames](https://www.geonames.org/), CC BY 4.0. United Kingdom: Contains Royal Mail data © Royal Mail copyright and database right |
| European regions                    | © European Union - GISCO, postal code point dataset, Licence CC BY-SA 4.0; NUTS and municipalities © European Union, Eurostat        |
| German postcodes and municipalities | [OpenPLZ](https://www.openplzapi.org/), from OpenStreetMap, © OpenStreetMap contributors, ODbL 1.0                                   |

The dialog and the Settings section show the same credits.

What this means for you:

- **For your own use, nothing.**
- **When you pass a table with these columns on**, name the sources of the
  columns you added.
- Columns from **European regions** (CC BY-SA 4.0) or **German postcodes and
  municipalities** (ODbL 1.0) come under share-alike licences: a table you
  pass on with them has to stay under the same terms.
- Columns from **GeoNames** or **BIS** need the credit only.

This is a plain reading of the licences, not legal advice.

## Command line and MCP

`octa --reference` (see [`--reference`](../cli/reference.md)) and the
[`reference_data`](../mcp/tools/reference_data.md) MCP tool do the same. Both
refuse while the download they need is switched off.
