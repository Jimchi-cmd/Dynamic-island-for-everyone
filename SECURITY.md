# Security policy

## Reporting a vulnerability

Please **don't open a public issue**. Use GitHub's private reporting instead: go to the repository's **Security** tab and choose **Report a vulnerability**. You'll get a reply within a week. Once a fix ships, you'll be credited unless you prefer not to be.

## Supported versions

Only the latest release gets security fixes.

## What's in scope

- **Secrets.** The Google Routes API key must stay in the OS keychain and must never reach a file, a log, a command line or any host other than `routes.googleapis.com`.
- **The local control channel** (`crates/islet/src/control.rs`): TCP on 127.0.0.1 only, guarded by a random token in a file only the user can read. Bypassing the token, or reaching the channel from another user account or the network, is a vulnerability.
- **Settings → island commands.** Settings may only run the allow-listed `islet` commands (`allowed()` in `src-tauri/src/lib.rs`), with arguments passed directly and never through a shell.
- **Updates.** In-app updates must be verified with the release's Ed25519 key (`tauri-plugin-updater`, with signed versions required). Installing an unsigned or downgraded update is a vulnerability.
- **The macOS MediaRemote helper**, which runs inside `/usr/bin/perl` and talks over a pipe.

Out of scope: problems that need an already-compromised account or machine, and third-party services (Google, Microsoft, Apple) themselves.
