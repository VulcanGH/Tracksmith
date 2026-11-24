# Built-in updates

Open **About → Updates**, **Settings → Updates**, or the tray's **Check for updates**.
Checks are manual and use stable GitHub Releases from `VulcanGH/tracksmith`.
Drafts, prereleases and older versions are excluded. There is no automatic installation.
Until the repository has a published stable release, the check reports that no public
release is available. No GitHub token or media information is sent.

When a newer release exists, review its notes and choose **Install/Update and restart**.
Save or revert editor/rule drafts, save or discard a batch preview, finish active media
work and open/dismiss queued Explorer selections first. Update preparation blocks new
media actions. It never starts or interrupts a metadata write.
Close other Tracksmith browser tabs before installing an update; the browser session
lock prevents a second tab from bypassing draft checks. Other instances using the
same executable must also be closed. Unsupported browsers can still use the full
downloaded installer manually.

Installed copies use `Tracksmith-VERSION-Setup.exe` and request Windows administrator
approval. Portable copies use `Tracksmith-VERSION-portable.zip`, extract only the root
`tracksmith.exe` entry and require write access to the executable's folder. Portable
documentation stays in the downloaded package; the updater replaces the self-contained
executable, not user configuration. Use the full ZIP to refresh accompanying documents.

Both paths validate the exact repository/asset name, release version, download size and
SHA-256 digest from GitHub's HTTPS API. A separate worker waits for Tracksmith to exit
without terminating it, applies the update, and reopens the application with the same
configuration directory and port. Portable replacement restores the old executable if
its new version cannot be verified. Installer cancellation/failure is reported after the
existing application is reopened, where possible. No media files or user settings are
part of the update package. The latest result is recorded in `update-result.json` in
the configuration directory and shown in Updates after restart.

## Publishing compatible releases

1. Use a stable `vMAJOR.MINOR.PATCH` tag matching Cargo/package versions.
2. Build with `scripts/build.ps1 -Release`, then `scripts/package.ps1`.
3. Upload the matching Setup EXE, portable ZIP and SHA256SUMS text file to the same
   GitHub release. The built-in updater requires the SHA-256 asset digest supplied
   by GitHub; the extra sums file supports manual verification.
4. Publish the release only after reviewing its assets. Draft/prerelease releases
   cannot be installed by the updater. The release workflow prepares a draft for review.
5. Validate a real version-to-version update once the first release is published.

The trust source for unsigned builds is the configured GitHub repository over HTTPS,
matching UFCM's release-checksum model. Checksums alone do not provide an independent
publisher identity if that release account is compromised. Signed releases can enable
XFold-style certificate pinning: set build-time `TRACKSMITH_UPDATE_CERT_SHA256` to the
64-hex SHA-256 of the signing certificate's DER bytes. The worker then requires valid
Authenticode signatures with that exact publisher certificate for installers/portable
executables. A certificate change requires an intentional pin transition/release plan.
`TRACKSMITH_UPDATE_REPOSITORY` can override the fixed repository at build time for a fork.
No runtime request can provide an arbitrary repository, download URL or executable path.

References: [GitHub release asset digests](https://docs.github.com/en/rest/releases/assets),
[Inno Setup update arguments](https://jrsoftware.org/inno-setup/documentation/topic_setupcmdline.htm).
