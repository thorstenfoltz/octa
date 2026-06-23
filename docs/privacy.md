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
  choose (Anthropic, OpenAI, or Google Gemini) so it can answer; that data
  leaves your machine and is handled under that provider's own privacy terms.
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

No other network activity occurs.

**Contact:** report concerns via
[GitHub Issues](https://github.com/thorstenfoltz/octa/issues).
