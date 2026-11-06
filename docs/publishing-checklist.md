# Publishing checklist

Run through this for every release. The first one is v0.1.0.

## One-time setup

- [ ] Repository **Settings → General**: description, website, topics (`dynamic-island`, `overlay`, `rust`, `tauri`, `macos`, `windows`, `linux`).
- [ ] **Settings → Code security**: turn on **Private vulnerability reporting** (`SECURITY.md` points to it), Dependabot alerts and secret scanning.
- [ ] **Settings → Branches**: protect the default branch and require the CI checks.
- [ ] Update signing key (see [signing.md](signing.md) §1). Recommended before the first release, because installs from an unsigned release can't update in place later.
- [ ] Optional: macOS notarization and Windows signing (signing.md §2–3).

## Each release

1. [ ] `main` is green in CI on all three OSes, including the perf budgets.
2. [ ] The latest **Release** dry run on `dev` is green (it runs when packaging changes, or start it from the Actions tab).
3. [ ] Bump the version in `Cargo.toml` (`workspace.package.version`), `package.json` and `src-tauri/tauri.conf.json`. The tag must match all three, or the release fails.
4. [ ] Move the `CHANGELOG.md` notes from "unreleased" to the version and date.
5. [ ] Regenerate third-party licenses: `scripts/third-party-licenses.sh` (needs `cargo install cargo-about --features cli`).
6. [ ] Merge to `main`, then tag and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

7. [ ] Wait for the **Release** workflow. It builds every installer, checks sizes, writes `SHA256SUMS` (and `latest.json` when update signing is set up), and opens a **draft** release.
8. [ ] Smoke-test the draft's downloads on real machines:
   - [ ] Windows 11: install, welcome screen, island appears, music from Spotify or a browser, a Bluetooth headset connect pop-up, an alarm rings, Wi-Fi list.
   - [ ] macOS (notched MacBook if possible): drag to Applications, Gatekeeper flow from [install.md](install.md), notch wrap, Now Playing with artwork, Bluetooth and Location prompts.
   - [ ] Linux: the `.deb` on Ubuntu (GNOME Wayland), the AppImage on another distro, MPRIS music and the Wi-Fi list.
   - [ ] `islet --version` shows the new version, and the old version updates in place (when signed).
9. [ ] Edit the draft notes (start from the CHANGELOG section), then **Publish**.
10. [ ] After publishing: check that `releases/latest/download/latest.json` resolves (signed releases), and announce.

## If something goes wrong

- A bad release: mark it as a pre-release or delete it, fix, and tag a new patch version. Never re-use a tag.
- A leaked signing key: rotate it (signing.md). Installs that trusted the old public key need a manual reinstall.
