# Privacy Policy

Octa is a desktop application for viewing and editing tabular data files.

**Octa is fully offline by default.** Every feature that goes online is off
until you switch it on or use it, with one exception: the version check asks
GitHub once per launch whether a newer release exists, and **Settings ->
Updates** turns that off too. Octa collects no personal data, sends no
telemetry or analytics, and does no remote logging or crash reporting. Your
files are opened and edited locally, and are not sent anywhere unless you
enable one of the optional features described below.

**Only three features use a language model**, and all three are listed below:
the Chat assistant and the two plain-language **Ask** boxes. Everything else
Octa calls analysis, including the Join key finder, Join diagnostics, near
duplicate and fuzzy matching, PII detection, outlier detection, clean-up
suggestions, the Summary and the HTML report, is ordinary arithmetic running on
your own machine. It consults no model, sends nothing anywhere, and gives the
same answer every time.

There are a few optional outbound network calls, all of which you control:

- **AI assistant (Chat).** The in-app Chat assistant is off until you enable it
  and configure a provider. Once you use it, your prompts and the contents of
  the files and tables it works with are sent to the language-model provider you
  choose (Anthropic, OpenAI, Google Gemini, any OpenAI-compatible endpoint
  you point it at, or GitHub or OpenAI through their own command-line program
  when you use a subscription profile) so it can answer; that data leaves your machine and is
  handled under that provider's own privacy terms.
  If you instead point the assistant at a local model (Ollama on your own
  machine), nothing leaves your machine. Any API key you enter is stored locally
  on your device (operating-system keychain where available, otherwise Octa's
  settings file) and is sent only to its provider.
- **Ask (search bar and SQL panel).** The plain-language **Ask** boxes send one
  request to the same provider your chat profile names. They send your question
  plus the active table's **column names, their types and its row count** - not
  the cell values. Both are inert until a chat profile exists.
- **Classify with Jev.** Off until you switch it on in Settings. When you run
  it, the cells of the columns you chose, your question and your categories or
  levels are sent to TypeSafe AI, whose servers are in the United States only
  and which names no period after which data is deleted; that data leaves your
  machine and is handled under TypeSafe's own terms. Each distinct value is
  sent once, and the dialog says how many before it sends them. The API key
  you enter is stored locally and sent only to TypeSafe.
- **Online data (reference downloads).** Five downloads for the **Reference
  data** dialog, each off until you switch it on under **Settings -> Online
  data**. When on, Octa downloads public files and stores them as Parquet in
  the `reference` folder next to its settings; it sends nothing about your
  data, only an ordinary request with Octa's version in the `User-Agent`. It
  checks again at the interval you set (default once a month) and skips a
  file that has not changed. **Delete downloaded data** removes a download
  from your computer.
  - *Exchange rates*: `data.bis.org` (Bank for International Settlements),
    about 5 MB, about 2 MB stored.
  - *Inflation*: `data.bis.org`, under 1 MB.
  - *Postcodes*: `download.geonames.org`, one file per country the first
    time you use it, 0.4 to 15 MB each.
  - *European regions*: `gisco-services.ec.europa.eu` and `ec.europa.eu`
    (Eurostat), about 75 MB, about 2.5 MB stored.
  - *German postcodes and municipalities*: `raw.githubusercontent.com`
    (OpenPLZ), about 55 MB, under 100 KB stored.
- **Update check.** Octa asks GitHub for the address of its latest release
  (`https://github.com/thorstenfoltz/octa/releases/latest`) to compare
  versions: once per launch by default, and whenever you choose **Help -> Check
  for updates**. The request carries Octa's version in the `User-Agent` header
  and nothing else, and it downloads and installs nothing by itself. Turn
  **Settings -> Updates -> Check for updates at start** off to limit it to the
  menu entry. Copies installed from the Microsoft Store make only the
  launch-time request, to tell you a new version exists; the Store installs it,
  so the menu entry does not exist there. The release notes
  Octa shows you after an upgrade cost no request at all: they are built into
  the binary.
- **Map tiles.** When you open a geographic file in **Map** view, Octa fetches
  background map tiles from OpenStreetMap (`tile.openstreetmap.org`). Switch the
  Map view to geometry-only to avoid this.
- **Cloud object storage.** If you add a connection to Amazon S3, Azure Blob
  Storage or Google Cloud Storage, Octa contacts that service to list and
  download the objects you open, and to upload files if you turn writing on
  (it is off by default). The destination is the account you configure, not
  ours. Credentials you enter are stored locally on your device
  (operating-system keychain where available, otherwise Octa's settings file)
  and are sent only to that provider.
- **Database connections.** If you add a connection to a database server
  (PostgreSQL, MySQL, SQL Server, Oracle, Redshift, ClickHouse, Exasol,
  Trino, Athena, Snowflake, Databricks or BigQuery), Octa connects to the server you specify to list
  tables and run the queries you write. Passwords and tokens are stored locally
  as above. Where you choose browser or CLI sign-in, authentication goes to
  that vendor's identity service.

The update check runs on its own without being switched on; the reference
downloads run on their own only once you have switched them on, at the
interval you set. Everything else waits for you to act: a connection you
configure, a Map view you open, a chat profile you set up. Each talks only to the
service you nominate. Octa has no servers of its own and no account system, so
none of this data reaches the developer. No other network activity occurs.

## Data sources and licences

Octa ships none of the reference data. Each copy of Octa downloads it from
the publisher once you switch that download on.

| Download                            | Source and licence                                                                                                                   |
|-------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| Exchange rates, Inflation           | Source: BIS (Bank for International Settlements), [data.bis.org](https://data.bis.org/help/legal)                                    |
| Postcodes                           | [GeoNames](https://www.geonames.org/), CC BY 4.0. United Kingdom: Contains Royal Mail data © Royal Mail copyright and database right |
| European regions                    | © European Union - GISCO, postal code point dataset, Licence CC BY-SA 4.0; NUTS and municipalities © European Union, Eurostat        |
| German postcodes and municipalities | [OpenPLZ](https://www.openplzapi.org/), from OpenStreetMap, © OpenStreetMap contributors, ODbL 1.0                                   |

For your own use you need to do nothing. When you pass on a table with columns
added from these sources, name the sources. Columns from European regions (CC
BY-SA 4.0) or German postcodes and municipalities (ODbL 1.0) come under
share-alike licences, so a table passed on with them has to stay under the
same terms; columns from GeoNames or BIS need the credit only. This is a plain
reading of the licences, not legal advice. See
[Reference data](usage/reference-data.md#sources-and-licences).

**Contact:** report concerns via
[GitHub Issues](https://github.com/thorstenfoltz/octa/issues).
