# Languages

Octa's interface, menus, dialogs, the status bar, the SQL and
multi-search panels, the right-click menus, and the Settings dialog
(including its hover tooltips), can be shown in any of 17 languages.

Change it under **Settings → Appearance → Language**. The switch is
**live**: the interface updates on the next frame, with no restart.

## Available languages

| Code | Language   |
|------|------------|
| `en` | English    |
| `de` | Deutsch    |
| `es` | Español    |
| `fr` | Français   |
| `it` | Italiano   |
| `nl` | Nederlands |
| `pt` | Português  |
| `pl` | Polski     |
| `sv` | Svenska    |
| `da` | Dansk      |
| `no` | Norsk      |
| `fi` | Suomi      |
| `tr` | Türkçe     |
| `id` | Indonesia  |
| `vi` | Tiếng Việt |
| `ro` | Română     |
| `hu` | Magyar     |
| `cs` | Čeština    |

The chosen code is stored as `language` in your
[`settings.toml`](settings.md).

## Good to know

- **Machine-generated translations.** The non-English catalogs are
  machine-translated and may be refined over time. English is the
  master; anything not yet translated falls back to the English string
  rather than showing a blank or a key.
- **Newest languages are core-UI first.** The five most recent additions
  (Indonesian, Vietnamese, Romanian, Hungarian, Czech) cover the core
  interface; a few long help paragraphs still fall back to English until
  they are translated.
- **What stays in English on purpose.** Technical identifiers are not
  translated, so they read the same in every language: format names
  (Parquet, JSON, …), database engine names, theme names (Nord,
  Dracula, …), schema-export targets, and low-level reader error
  messages.
- **Number formatting is separate.** The decimal mark and digit
  grouping are controlled by **Number style** (English `1,234.56`
  vs European `1.234,56`) in [Settings → Table View](settings.md#table-view),
  independent of the UI language.

## Scripts not yet offered

Only Latin-script languages whose accented characters the bundled font
already covers are available. Cyrillic, CJK (Chinese / Japanese /
Korean), and right-to-left scripts (Arabic, Hebrew) are out of scope
for now: they need bundled fonts and, for RTL, layout work that Octa
does not yet do.

## See also

- [Settings reference](settings.md) lists every setting, including
  **Language** and **Number style**.
- [Non-English data](../getting-started/supported-formats.md) is handled
  separately from the UI language, encodings and separators are detected
  per file.
