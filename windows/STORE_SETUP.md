# Microsoft Store setup

Two ways to get a release into the Store. **Manual is the working path today**;
automation is blocked on an account problem described in section B.

Portal labels shift and the account is German; German labels are given where
they were confirmed in the portal.

## Where things stand

- App reserved as **Octa Data Viewer** (the name "Octa" was taken). Store ID
  `9PF9BVRT9PX4`.
- `AppxManifest.xml` carries the real identity and declares all 32 interface
  languages, which is what makes Partner Center offer 32 listing columns.
- Nothing is published yet.

## A. Manual path (works, no Azure required)

### 1. Build the package

    ./windows/build-msix.sh

Fetches the newest published release, verifies its checksum, packs an MSIX and
verifies it by unpacking it again. The result is `windows/octa-<version>.0.msix`
(gitignored). Pass a tag to pin a version.

The package is **unsigned**, which is correct: Partner Center accepts unsigned
packages and the Store signs them for distribution.

### 2. Upload it

Partner Center -> the submission -> **Pakete** -> drag the `.msix` in.
Uploading is not submitting; the package sits in the draft and gets validated,
so this is also how you find out whether the identity is right.

### 3. Fill the listings

Export the CSV **after** the package is uploaded, otherwise it has no language
columns:

    Partner Center -> app overview -> Store-Einträge -> Export listing
    ./scripts/build-store-listing.py ~/Downloads/listingData-*.csv ~/Downloads/octa-store
    Partner Center -> Import listings -> Import folder -> octa-listing

That fills all 32 language listings in one import. Content lives in
`docs/assets/store/listings/`; see `docs/assets/store/INDEX.md`.

### 4. The three sections the CSV does not cover

- **Eigenschaften**: category *Entwicklertools*. For the `runFullTrust`
  restricted-capability prompt, paste
  `docs/assets/store/runfulltrust-justification.txt`.
- **Altersfreigaben**: IARC questionnaire. Octa rates 3+.
- **Preise und Verfügbarkeit**: Grundpreis = **Kostenlos**. Ignore the
  per-market *Einzelhandelspreis* fields.

Support info: privacy policy `https://thorstenfoltz.github.io/octa/privacy/`
(answer **Ja** to the personal-information question, and note that declaring
`runFullTrust` makes Partner Center force that answer anyway); website
`https://thorstenfoltz.github.io/octa/`; support contact
`https://github.com/thorstenfoltz/octa/issues`. Phone and address are optional
for individual developers.

### 5. Submit

Certification typically takes one to three days.

## Shipping a new version (the steady state)

Once the first submission is live, an update is three steps:

1. Cut the GitHub release as normal (**Release** workflow, new version).
2. `./windows/build-msix.sh` - no argument, it picks up the release you just
   cut.
3. Partner Center -> **Update** / new submission -> **Pakete** -> upload the
   new `.msix`, replacing the old one -> submit.

Everything else **persists across submissions** and does not need redoing:
Store listings, age rating, pricing, category, the `runFullTrust`
justification, privacy and support info.

Three things that do force extra work:

- **The version must be strictly higher** than the published one. The manifest
  takes `<tag>.0`, so a `0.16.0` tag becomes `0.16.0.0`. You cannot re-upload
  or reuse a version, even a withdrawn one.
- **If you added or removed a locale**, `AppxManifest.xml` must be updated to
  match `locales/` first. Partner Center reads the language list off the new
  package and will offer a listing column for each new one, which then has to
  be filled or the submission is incomplete. Re-run the export -> build ->
  import cycle from section A step 3.
- **If the descriptions changed** (a feature worth mentioning landed), edit
  `docs/assets/store/listings/*.toml` and re-run the same cycle. The listing
  and the package are separate submissions-worth of work; changing one does
  not require touching the other.

Certification runs again on every update, typically one to three days.

## B. Automated path (removed from CI)

There **used to be** a `store-publish` job in `release.yml` that ran
`msstore publish` behind a `publish_to_store` input. It was deleted: it could
never run, because it needs four repository secrets
(`PARTNER_CENTER_TENANT_ID`, `_CLIENT_ID`, `_CLIENT_SECRET`, `_SELLER_ID`)
that cannot be obtained on this account. Recover it from git history if the
blocker below is ever resolved.

**The blocker.** Those credentials require an Entra tenant where you are a
global admin. The Partner Center account is registered to a personal Microsoft
account, which has none, and self-service tenant creation fails: both
`portal.azure.com` and `entra.microsoft.com` reject the sign-in with AADSTS50020
("account not in tenant Microsoft Services") after an AADSTS50058 silent-token
failure. Ruled out as causes: multi-account browser sessions, third-party
cookie blocking, browser choice (Firefox and Edge both fail), and portal choice.
It is an account provisioning state, not something clickable.

Routes if you want to revisit it:

1. Partner Center -> **Hilfe und Support**, asking about associating an Entra
   tenant with an MSA-registered developer account. Free, and they can see the
   account state.
2. Sign up at `https://azure.microsoft.com/free`, which provisions a default
   directory. Requires a card for identity verification; no charge.

Once a tenant exists: create a work admin (`admin@<tenant>.onmicrosoft.com`,
Global Administrator), use **only that account** for the Entra steps, associate
it under **Kontoeinstellungen -> Mandanten**, then add an Azure AD application
named `Octa Store Publisher` with the **Manager** role under **User
management**. Then restore the job and add the four secrets.

Cost/benefit: automation saves roughly two minutes per release over the manual
path. It is not worth blocking a release on, which is why the dead job was
removed rather than left switched off.

## Notes

- `build-msix.sh` builds Microsoft's cross-platform packer
  (`microsoft/msix-packaging`) into `windows/.msix-tools/` on first run, since
  `makeappx.exe` is Windows-only. It patches the upstream C++14 pin to C++17,
  without which the bundled build fails against modern system ICU headers.
- The in-app updater is suppressed for Store copies via
  `src/platform.rs::is_store_packaged()`; the Store delivers their updates.
- The Store listing section in `docs/getting-started/installation.md` is
  commented out until the app is actually live. Uncomment it then.
- If `crt-static` (`.cargo/config.toml`) ever fails to link on Windows, the
  fallback is bundling the VC++ runtime DLLs into the MSIX or declaring a
  `Microsoft.VCLibs.140.00` dependency. Record the choice here.
