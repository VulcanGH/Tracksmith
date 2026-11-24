# Security and data handling

Tracksmith runs on Windows and serves its interface on IPv4 loopback only. API
requests require a per-process credential and exact local Host/Origin checks.
Media access comes from native selection, explicit configured locations or retained
history. Updates contact the configured GitHub Releases repository only on request.
Media paths and inspection results are not sent to an external service.

The source and Windows packages are MIT-licensed. The first release is unsigned;
updates verify HTTPS release metadata, the expected asset, version, size and SHA-256.
The GitHub release account is the trust source for these unsigned updates. Optional
publisher-certificate pinning is documented in [UPDATES.md](docs/UPDATES.md).

Saves change original files in place. Verification checks requested and unrelated
metadata; restoration records restore verified metadata values, not damaged file
contents or exact original bytes. Keep backups appropriate to your media library.
Inspection utilities are external prerequisites and should be kept up to date.

For a security issue, use GitHub's private vulnerability reporting if enabled on
this repository. Public bug reports should omit session URLs, session.json,
credentials and private media paths. Include only a minimal anonymised reproduction.
