# Signing releases

Unsigned builds work but users see warnings: SmartScreen on Windows, and Gatekeeper on macOS, where users must click "Open Anyway". This page lists what to set up and where. **Never commit keys or certificates.** They go in the repository's GitHub **Settings → Secrets and variables → Actions**, and `.gitignore` refuses common key files.

## 1. Update signing (free, recommended first)

In-app updates are verified with an Ed25519 key. Without it, releases still build, but Settings sends users to the download page instead of updating in place.

```bash
npx tauri signer generate -w ~/.tauri/islet-updater.key
```

Keep the private key and its password somewhere safe, such as a password manager. If you lose them, existing installs can't verify future updates. Then add to GitHub:

| Name | Kind | Value |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Secret | the contents of `islet-updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | its password |
| `ISLET_UPDATER_PUBKEY` | **Variable** (not secret) | the contents of `islet-updater.key.pub` |

The release workflow then builds signed update bundles, writes `latest.json`, and fails if any platform's bundle or signature is missing. Updates require the signed version to match, which blocks downgrade attacks.

## 2. macOS: Developer ID and notarization

Needs an [Apple Developer Program](https://developer.apple.com/programs/) membership ($99 a year).

1. In Xcode or the developer portal, create a **Developer ID Application** certificate and export it as a `.p12` with a password.
2. Create an **app-specific password** for your Apple ID at [appleid.apple.com](https://appleid.apple.com).
3. Add these secrets:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i cert.p12` output |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | the app-specific password |
| `APPLE_TEAM_ID` | your 10-character team ID |

The release workflow passes them to the Tauri bundler only when they're set. It signs the app, including the island and its MediaRemote helper, and notarizes the `.dmg`.

## 3. Windows: Authenticode

Choose one:

- **MSIX package:** `scripts/msix.ps1` signs the `.msix` with the certificate in the `MSIX_PFX_BASE64` secret (a base64-encoded `.pfx`) and `MSIX_PFX_PASSWORD`; the package's Publisher becomes the certificate's subject. Without them, CI and releases sign it with a throwaway self-signed certificate (`CN=Islet Test`) and ship its public `.cer`, which testers trust once ([install guide](install.md#installing-the-msix)). The private key is created on the runner and never saved.
- **[SignPath Foundation](https://signpath.org/)**: free code signing for open-source projects, after approval. **The release workflow is already wired for it**: once these are set, every release's `.msi` and setup `.exe` are sent to SignPath, come back signed, and (with update signing on) get fresh update signatures:

  | Name | Kind | Value |
  |---|---|---|
  | `SIGNPATH_API_TOKEN` | Secret | a CI user's API token from SignPath |
  | `SIGNPATH_ORGANIZATION_ID` | Variable | your SignPath organization id |
  | `SIGNPATH_PROJECT_SLUG` | Variable (optional) | the project's slug (default `islet`) |
  | `SIGNPATH_POLICY_SLUG` | Variable (optional) | the signing policy's slug (default `release-signing`) |

  In SignPath, link the project to this GitHub repository (their "trusted build system" setup) and give it an artifact configuration for a zip holding the installers, for example:

  ```xml
  <?xml version="1.0" encoding="utf-8"?>
  <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
    <zip-file>
      <pe-file path="*-setup.exe"><authenticode-sign /></pe-file>
      <msi-file path="*.msi"><authenticode-sign /></msi-file>
    </zip-file>
  </artifact-configuration>
  ```

  To apply: [signpath.org/apply](https://signpath.org/) with the repository link. They check that the project is open source and that releases are built by CI from the public code, which this repository does.
- **[Azure Artifact Signing](https://azure.microsoft.com/products/artifact-signing)** (formerly Trusted Signing): a low monthly fee. Set `bundle.windows.signCommand` in `src-tauri/tauri.bundle.json` to its `signtool` invocation and add the Azure credentials as secrets.
- A traditional OV or EV certificate from a certificate authority, used through `signCommand` in the same way.

SmartScreen warnings fade as a signed app builds reputation. EV certificates skip that wait.
