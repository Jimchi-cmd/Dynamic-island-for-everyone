# Code signing policy

Windows releases (the `.msi` and setup `.exe`): free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/). *(Applied for; until it is granted, Windows builds are unsigned or, for the `.msix`, signed with a test certificate.)*

## What gets signed

Only what GitHub Actions builds from this public repository's code, for a tagged release (`.github/workflows/release.yml`). Nothing built on a personal computer is signed. Each signing request is approved by hand before it runs.

## Team roles

| Role | Who |
|---|---|
| Authors and committers | [Jimchi-cmd](https://github.com/Jimchi-cmd) |
| Reviewers | [Jimchi-cmd](https://github.com/Jimchi-cmd) |
| Approvers (sign releases) | [Jimchi-cmd](https://github.com/Jimchi-cmd) |

Everyone with any of these roles uses multi-factor authentication on GitHub and on SignPath. Contributions from others come in as pull requests, which a committer reviews before they are merged.

## Privacy

Islet doesn't send anything about you or your computer anywhere unless you turn on a feature that needs the network, and then only what that feature needs. There are no accounts, analytics or telemetry. The details are in the [README's Privacy section](README.md#privacy).

The phone app and the update key are signed with keys that only the maintainer holds ([docs/signing.md](docs/signing.md)).
