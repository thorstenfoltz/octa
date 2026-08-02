# Privacy Policy

Octa is a desktop application for viewing and editing tabular data files.

**Octa is fully offline by default.** It collects no personal data, sends no
telemetry or analytics, and does no remote logging or crash reporting. Your
files are opened and edited locally, and are not sent anywhere unless you
enable one of the optional features described below.

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
- **Update check.** When you choose **Help -> Check for updates**, Octa queries
  the GitHub releases API
  (`https://api.github.com/repos/thorstenfoltz/octa/releases`) to compare
  versions. Copies installed from the Microsoft Store do not do this; the Store
  handles their updates.
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
  (PostgreSQL, MySQL, SQL Server, Redshift, ClickHouse, Exasol, Snowflake,
  Databricks or BigQuery), Octa connects to the server you specify to list
  tables and run the queries you write. Passwords and tokens are stored locally
  as above. Where you choose browser or CLI sign-in, authentication goes to
  that vendor's identity service.

Every item above is off until you configure it, and each one talks only to the
service you nominate. Octa has no servers of its own and no account system, so
none of this data reaches the developer. No other network activity occurs.

**Contact:** report concerns via
[GitHub Issues](https://github.com/thorstenfoltz/octa/issues).
