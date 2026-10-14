# 04: Release workflow and published guest image

**What to build:** Pushing a `v*` tag publishes release binaries for Linux x86_64, Linux arm64, and macOS arm64 with checksums, and the guest image for amd64 and arm64 under one multi-architecture tag in GitHub Container Registry. New DevVMs use that image by default and pull it on their first start.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] A `release` workflow on `v*` tags fails when the tag does not equal `v` plus the crate version.
- [ ] Binaries are built on native hosted runners and published in a GitHub Release with a SHA-256 checksum file.
- [ ] The image is built natively per architecture, pushed by digest, and combined into `ghcr.io/risakirisu/devvm:<version>` and `:latest`; image jobs have read access to contents and write access to packages, the release job write access to contents; disk usage is printed and the build fits the runner.
- [ ] The default image is `ghcr.io/risakirisu/devvm:<binary version>`; the configuration's image setting overrides it with another reference or a local tar.
- [ ] The readme documents installing from a release (clone the repository for the shared root, download the binary with `curl` or `Invoke-WebRequest`, run `setup`) and the GitHub Actions concepts in GitLab CI terms.
- [ ] Maintainer: after the first release, the package is set to public; a new DevVM created from the default image on Linux and macOS passes ticket 03's guest checks.
