---
paths:
  - ".github/**"
  - "windows/**"
  - "Dockerfile"
  - "install.sh"
  - "install.bat"
  - "install.ps1"
  - "deny.toml"
  - "about.hbs"
  - "src/app/update_check/**"
---
# Packaging, CI, release and licensing

## CI Pipeline

PRs to `master` run four jobs: `test` (fmt + clippy + cargo test, one shared `Swatinem/rust-cache@v2` target dir), `licenses` (`cargo deny check licenses`), `megalinter` (shell/security/markdown via `ghcr.io/oxsecurity/megalinter-rust:v10`), and `db-live`. Clippy/rustfmt live in `test`, not megalinter (no Rust cache there, ~1000s/PR). **`megalinter` runs first and hard-gates `test`** (`test.needs: [changes, megalinter]`): a lint failure skips the Rust job instead of burning ~15 min of compute. `licenses` stays parallel (`needs: [changes]`).

**`db-live`** (`needs: [changes, test]`, `if: needs.changes.outputs.db == 'true'`) runs `tests/db_live_tests.rs` against **Postgres 17 + MySQL 8 service containers**. Gated on a `db` paths filter (`src/db/**`, `tests/db_live_tests.rs`) because standing up two servers is only worth it when the connectors change; gated on `test` so it never boots a database for code that does not compile, and so `Swatinem/rust-cache` (`shared-key: build` on both, `save-if: false` here) restores `test`'s warm target dir. No MSSQL: the image is ~1.5 GB and slow to go healthy, and the job already has to reclaim ~20 GB of preinstalled runner toolchains to fit the build. The step **greps the log for `skipped: OCTA_TEST_(POSTGRES|MYSQL)_URL` and fails the job on a hit** - the suite passes as green no-ops when an env var is missing, so without that guard a typo would produce a passing job that connected to nothing. The tests create and drop their own databases, hence the superuser accounts.

`release.yml` (`workflow_dispatch`) intentionally does **not** re-run `cargo test` (PR CI already validated the merged commit). Jobs: `build-linux` (+ AppImage), `build-windows`, `build-macos` (aarch64 only - the Intel job was dropped as too slow), `publish` (needs all builds), `aur-publish`, and `docker-publish` (runs **in parallel** with the builds, no `needs:` - the Dockerfile builds from source so it never depended on the release artifacts; `docker/*` actions pinned to their first Node.js 24 majors so no `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` opt-in is needed). One boolean input gates the publishing half: **`build_only` (default false) skips `publish` and `docker-publish`**, and `aur-publish` inherits the skip through `needs: [publish]`; the three build jobs still run, so a `build_only` dispatch produces every artifact without cutting a public release. There is **no Store publishing in CI**: the `store-publish` job and its `publish_to_store` input were removed, since the Partner Center credentials they needed are unobtainable on this account (see `windows/STORE_SETUP.md`) and `windows/build-msix.sh` packs a Store MSIX locally instead.

## Microsoft Store

Published as **Octa Data Viewer** (the name "Octa" was taken), Store ID `9PF9BVRT9PX4`, listing https://apps.microsoft.com/detail/9PF9BVRT9PX4. Submitted manually (`windows/build-msix.sh` -> Partner Center); the docs point Windows users there first since the Store-signed build skips SmartScreen. **Full runbook `windows/STORE_SETUP.md`; asset inventory `docs/assets/store/INDEX.md` - read those before touching either script.** Two local scripts do the work that CI otherwise would:

- **`windows/build-msix.sh`** packs a Store MSIX from a *published GitHub release*, on Linux: download, verify against `SHA256SUMS`, stage (six `magick`-resized logos + version-substituted manifest), pack, then unpack again to re-verify every block-map hash and `cmp` the exe against the release binary. `makeappx.exe` is Windows-only, so it builds Microsoft's `msix-packaging` `makemsix` into `windows/.msix-tools/` on first run - that build needs **`MSIX_PACK=on`** (off by default, else the binary can only unpack) and a **C++14 -> C++17 bump in two `CMakeLists.txt`**. Output `windows/octa-<ver>.0.msix`, unsigned (correct: the Store signs after certification). `OCTA_MSIX_TEST=1` adds `OID.2.25.311729368913984317654407730594956997722=1` to `Identity/Publisher`, which Windows 11 demands before `Add-AppxPackage -AllowUnsigned` will take it - sideload-test only, that OID is part of the package identity and must never reach Partner Center.
- **`scripts/build-store-listing.py`** fills the Partner Center listing CSV. The export has a `default` column plus one per language **detected in the uploaded package** (hence the 32 `<Resource Language>` entries in `AppxManifest.xml`); blank cells inherit `default`, so one English set covers all 32 - which is why the script **blanks** any cell `_default.toml` owns (a filled cell beats `default`). Content in `docs/assets/store/listings/<code>.toml` (31 files, native, informal; `en-us` inherits `_default.toml`; `sr` -> `sr-cyrl.toml`). It drops the Xbox/Holographic/SurfaceHub/mobile rows (453 -> 225) but **never trailer rows** (deleting one deletes the asset in Partner Center), and leaves `Field`/`ID`/`Type` untouched.

Store *listings* are not in the package; only the supported-language list is.

## Licensing

MIT. The `licenses` CI job enforces every transitive license against `deny.toml`'s allowlist; copyleft (AGPL/GPL/LGPL/SSPL) excluded. Artifacts: `THIRD_PARTY_LICENSES.md` (per-crate index, `cargo about generate about.hbs --output-file THIRD_PARTY_LICENSES.md`), `licenses/<SPDX-id>.txt` (canonical text per identifier, hand-curated, add a file when a new license family enters the tree), and **`NOTICE`** (hand-maintained; the bundled material that is NOT a crate, so `cargo about` cannot see it: the five OFL-1.1 fonts in `assets/`, with the copyright lines read out of each font's own name table. `NotoSansCJK-subset.otf` lost its licence records to subsetting, so NOTICE is the only place they exist).

**Every distribution channel must copy all four**, and this is the part that silently rotted: `release.yml` used to package `LICENSE` alone while `install.sh`/`install.bat` copied the bundle behind `if exist` guards, so the installers skipped it without a word and every published release shipped ~768 crates with no notices. The copies in `release.yml` are therefore unguarded on purpose. Channels: the three release archives + AppDir (`release.yml`), both AUR `PKGBUILD`s (`.github/aur*/`), `Dockerfile` (`/usr/share/octa/`), and `windows/build-msix.sh` (from `$ROOT`, like the logos, not from the release zip). Adding a channel means adding the copy.

## Auto-Update

Toolbar checker. `ureq` HTTP, `flate2`/`tar`/`zip` extraction. Logic in `src/app/update_check/` (+ `src/ui/toolbar/help_menu.rs` for the Help entry). `check_for_updates` GETs `releases/latest` on a thread; pure `parse_release(body) -> Result<String, String>` (unit-tested) returns the `v`-stripped `tag_name` and nothing else. The release **body is deliberately ignored**: the notes the user reads are baked into the binary, so a release published without a description costs nothing.

**Startup check and release notes are two independent features.** Two settings, both default **on**: `check_updates_on_start` (one background request per launch) and `show_release_notes` (the window). Neither reads the other.

*The check*: `update_loop::ui` fires it on the first frame it sees (it needs an `egui::Context` to repaint, so not `OctaApp::new`) guarded by `startup_update_started`, and `drain_startup_update_check` acts on the result once (`startup_update_seen`). `UpdateState::Available{version}` raises a `release.toast` `status_message`, so the check is never a silent no-op. `UpToDate` and `Error` stay silent: neither is worth interrupting a launch for, and a flaky network must not nag every time. **The check opens no window**, and `Available` carries no notes to open one with.

*The notes*: `src/app/dialogs/release_notes.rs` shows `release_notes.md`, `include_str!`'d at compile time. That is the same file CI hands to `gh release create --notes-file`, so the window and the GitHub release page cannot disagree, and it works offline, behind a firewall, and on a Store copy. `OctaApp::new` decides once via `should_show(show_release_notes, last_release_notes_version, VERSION)`: setting on, this version not already dismissed, and not `0.0.0-dev` (a dev build's notes describe the last real release, not the working tree). Body rendered by the shared `view_modes::markdown::render_pulldown`, title `release.whats_new`. The window is a fact about the version the user is **running**, never one on GitHub, so it has no Update button and needs no store note. Its "Do not show this again" checkbox writes `last_release_notes_version = VERSION` on the tick (not on close, so the `x` cannot lose the answer), making it once per **release** rather than per launch; closing without ticking means "not now" and it returns next start.

**Store handling**: knowing a release exists is useful on Store even though the *install* is impossible there, so `octa::platform::is_store_packaged()` (`src/platform.rs`, a cached `\WindowsApps\` path check, no Windows API) neither suppresses the check nor hides the Help entry. Its only effect is in `src/app/dialogs/update_dialog.rs`: on `Available`, the Store build drops **Update now** and shows `release.store` instead, because an MSIX under `WindowsApps` cannot overwrite itself. The release-notes window needs no such branch, having never offered an install. Status bar splits `Checking` from `Updating` (a spinner labelled "Updating..." for a read-only version query reads as a silent install).

i18n: appendable `[release]` section x32, which also holds the Settings section labels (same pattern as `[diagnostics]`).

## Windows Build

`build.rs` uses `winresource` to embed manifest/icon/metadata. `windows/octa.exe.manifest` controls UAC + DPI.

## Containers

Headless `Dockerfile` (multi-stage `rust:1-bookworm` -> `gcr.io/distroless/cc-debian12`) for CLI + `--mcp`. GUI libs (GTK/X11) compile but aren't dlopen'd headless, so the runtime drops them. `docker run -v $PWD:/data octa --schema /data/f.parquet`; MCP via `-i ... --mcp`. Podman-compatible. Docs `docs/cli/docker.md`. Alpine/musl rejected (DuckDB/rusqlite bundle C++).
