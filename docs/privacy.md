# Privacy Policy

Octa is a desktop application for viewing and editing tabular data files.

**Octa is fully offline by default.** It collects no personal data, sends no
telemetry or analytics, and does no remote logging or crash reporting. Your
files are opened and edited locally, and are not sent anywhere unless you
enable one of the optional features described below. The one exception is the
version check: Octa asks GitHub once per launch whether a newer release exists,
and you can switch that off under **Settings -> Updates**.

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
  choose (Anthropic, OpenAI, Google Gemini, or any OpenAI-compatible endpoint
  you point it at) so it can answer; that data leaves your machine and is
  handled under that provider's own privacy terms.
  If you instead point the assistant at a local model (Ollama on your own
  machine), nothing leaves your machine. Any API key you enter is stored locally
  on your device (operating-system keychain where available, otherwise Octa's
  settings file) and is sent only to its provider.
- **Ask (search bar and SQL panel).** The plain-language **Ask** boxes send one
  request to the same provider your chat profile names. They send your question
  plus the active table's **column names, their types and its row count** - not
  the cell values. Both are inert until a chat profile exists.
- **Update check.** Octa queries the GitHub releases API
  (`https://api.github.com/repos/thorstenfoltz/octa/releases`) to compare
  versions: once per launch by default, and whenever you choose **Help -> Check
  for updates**. The request carries Octa's version in the `User-Agent` header
  and nothing else, and it downloads and installs nothing by itself. Turn
  **Settings -> Updates -> Check for updates at start** off to limit it to the
  menu entry. Copies installed from the Microsoft Store are updated by the
  Store; the check there only tells you a new version exists. The release notes
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

The update check is the only one of these that runs on its own, once per launch,
and Settings turns it off. Everything else waits for you to act: a connection you
configure, a Map view you open, a chat profile you set up. Each talks only to the
service you nominate. Octa has no servers of its own and no account system, so
none of this data reaches the developer. No other network activity occurs.

**Contact:** report concerns via
[GitHub Issues](https://github.com/thorstenfoltz/octa/issues).
