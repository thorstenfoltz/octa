# Microsoft Store setup (one-time)

This is the manual setup that enables automated Store publishing. Do it once.
After it is done, every `Release` workflow run submits a new package on its own.
Portal labels shift over time; these are current as of 2026-06.

## 1. Reserve the app and capture its identity (Partner Center)

1. Go to <https://partner.microsoft.com/dashboard> -> **Apps and games**.
2. **+ New product**. For the product type pick **MSIX** (the other choice is
   **PWA app**; Octa is a packaged desktop app, so MSIX).
3. The name **Octa** is already reserved on this account, so select that
   reservation (no need to reserve it again).
4. Open the product -> **Product management** -> **Product identity**. Copy:
   - **Package/Identity Name** (e.g. `12345Publisher.Octa`)
   - **Publisher** (the `CN=...` string)
   - **Publisher display name**
5. Paste the first two into `windows/AppxManifest.xml` (`Identity Name` and
   `Publisher`), and confirm `PublisherDisplayName` matches. Commit that change.

## 2. Create Azure AD credentials for the API (Partner Center + Azure)

1. Partner Center -> gear icon -> **Account settings** -> **User management**
   -> **Azure AD applications** tab.
2. **Add Azure AD application** -> create a new one (or associate an existing
   app registration). Give it the **Manager** role.
3. Record:
   - **Tenant ID**
   - **Client ID** (Application ID)
4. Create a **client secret / key** for that application (the portal shows the
   secret value **once**, copy it immediately).
5. Find your **Seller ID**: Account settings -> **Identifiers** (or
   Organisation profile). It is a numeric ID.

## 3. Store the secrets in GitHub

Repo -> **Settings** -> **Secrets and variables** -> **Actions** ->
**New repository secret**, four times, with these exact names:

| Secret name | Value |
|-------------|-------|
| `PARTNER_CENTER_TENANT_ID` | Tenant ID from step 2 |
| `PARTNER_CENTER_CLIENT_ID` | Client ID from step 2 |
| `PARTNER_CENTER_CLIENT_SECRET` | Client secret from step 2 |
| `PARTNER_CENTER_SELLER_ID` | Seller ID from step 2 |

## 4. First listing (one manual pass, required before going live)

Automation submits the *package* from the first release; the *listing* below is
filled in by hand once.

1. In the product's submission, set **Properties**:
   - Category: **Developer tools** (or Productivity).
   - When prompted about the **runFullTrust** restricted capability, justify it:
     *"Octa is a packaged Win32 desktop application; full trust is required for
     a standard desktop executable."*
2. **Age ratings**: complete the IARC questionnaire (Octa has no mature content;
   it rates 3+).
3. **Store listings**: short description (< 200 chars) and long description
   (< 10,000 chars); see `docs/assets/store/INDEX.md` for drafts. Upload the
   screenshots and tile from that same folder.
4. **Pricing and availability**: Free; markets as desired.
5. **Privacy policy URL**: `https://thorstenfoltz.github.io/octa/privacy/`.
6. Either upload the first MSIX by hand, or (once section 3 is done) let the
   next `Release` run upload it. Then **Submit**. First certification is
   typically one to three days.

## 5. Confirm automation

1. Trigger **Release** with a numeric `version` and `publish_to_store: true`.
2. Watch the `store-publish` job authenticate and create a submission.
3. In Partner Center the new submission appears under the app and proceeds
   through certification. Subsequent releases need no manual step.

## Notes

- If `crt-static` (`.cargo/config.toml`) ever fails to link on Windows, the
  fallback is bundling the VC++ runtime DLLs into the MSIX payload or declaring
  a `Microsoft.VCLibs.140.00` dependency in the manifest. Record the choice here.
- Store updates reach users silently in the background; a running Octa updates
  on next launch.
