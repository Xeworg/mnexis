# Cutting a release

A release is a Git tag. The tag is the version (`v0.1.0` is version
`0.1.0`), the workflow builds the artifacts and publishes them with their
checksums, and the online installer installs whatever the latest release
carries.

## What a release publishes

| Asset | What it is |
| --- | --- |
| `mnexis-linux-x86_64.tar.gz` | The binary, `LICENSE`, `README.md` and a `VERSION` file. |
| `mnexis-linux-aarch64.tar.gz` | The same for the other supported platform. |
| `install.sh` | The installer, served by URL. |
| `SHA256SUMS` | The checksum of every asset above. |

The asset names carry no version: the release tag already namespaces them,
so the installer can ask for `releases/latest/download/<name>` or for a
pinned `releases/download/v0.1.0/<name>` with the same name.

## How to cut one

1. **Bump the version** in `Cargo.toml` (`version = "0.1.0"`) and let
   `Cargo.lock` follow (`cargo update -p mnexis` or a build).
2. **Commit it** on `main` with a conventional commit
   (`chore(release): 0.1.0`).
3. **Tag it**:

   ```sh
   git tag -a v0.1.0 -m "mnexis 0.1.0"
   git push origin main --tags
   ```

4. **Watch the workflow**: <https://github.com/Xeworg/mnexis/actions/workflows/release.yml>.
   It runs the whole suite, builds both platforms, packages them, verifies
   the checksums and publishes the release.

The tag must match the version in `Cargo.toml`: the packaging script
refuses to package a binary whose own version is not the one being
released, so a forgotten bump fails the workflow instead of shipping a
mislabelled artifact.

## Verify before you announce it

```sh
# The checksums of the published assets.
curl -fsSLO https://github.com/Xeworg/mnexis/releases/latest/download/SHA256SUMS
curl -fsSLO https://github.com/Xeworg/mnexis/releases/latest/download/mnexis-linux-x86_64.tar.gz
sha256sum -c SHA256SUMS

# The installer, in a throwaway HOME.
export HOME="$(mktemp -d)"
curl -fsSL https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | sh
mnexis --version
```

## What the checksums do, and what they do not

`SHA256SUMS` is published next to the artifacts, so anyone can check that
the file they downloaded is the file that was published. It protects
against a corrupted or truncated download.

It does **not** protect against a compromised release or a compromised
build pipeline: whoever can publish a release can publish a new checksum
with it. Closing that gap means signing the artifacts with a key whose
public part lives in this repository, so anyone can verify the signature
without trusting the release. That is a separate piece of work, on the
roadmap, and it is why the release notes say out loud that the artifacts
are not signed yet.

## Re-running a failed release

The publish step is the last one, so a failure in the build or the test
job leaves no release behind: fix the problem, push the fix, and re-run
the workflow for the same tag (GitHub keeps the run; use *Re-run all
jobs*).

If a release was published and has to be replaced, delete it first — the
workflow refuses to create a release that already exists:

```sh
gh release delete v0.1.0 --yes
git tag -d v0.1.0
git push origin :refs/tags/v0.1.0
git tag -a v0.1.0 -m "mnexis 0.1.0"
git push origin v0.1.0
```
