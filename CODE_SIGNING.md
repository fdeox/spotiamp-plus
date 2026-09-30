# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io),
certificate by [SignPath Foundation](https://signpath.org).

## What gets signed

Only files built from this repository by
[GitHub Actions](https://github.com/fdeox/spotiamp-plus/actions): the Windows
installer (`Spotiamp+_<version>_x64-setup.exe`) and the application it installs
(`spotiamp.exe`). Every release is approved by hand before it is signed.

## Team

- Committers and reviewers: [fdeox](https://github.com/fdeox)
- Approvers: [fdeox](https://github.com/fdeox)

Changes from other contributors are reviewed before they are merged. Everyone on
the team uses multi-factor authentication for GitHub and SignPath.

## Privacy

See the [privacy policy](PRIVACY.md). In short: no telemetry, and the app only
talks to Spotify, to GitHub to check for updates, and to the Winamp Skin Museum
when you open it.
