# Repository-Versioned Release Design

## Goal

Make `make release` update every repository-owned Kivo version, commit those
changes, and only then create the annotated release tag. Local firmware builds
must use the same version recorded in `.env`, without a `+dev` suffix.

## Version Model

Kivo uses two representations of one semantic version:

- Tag form: `vX.Y.Z`. This is stored in `.env` and `.env.example`, used as the
  Git tag, and reported as the firmware build ID.
- Package form: `X.Y.Z`. This is stored in npm, Python, Cargo, and Tauri package
  metadata and their repository-owned lock-file entries.

`.env` is the local release input and remains ignored by Git. `.env.example` is
the tracked onboarding template and records the same tag-form version as the
latest release commit.

The accepted format is exactly `v` followed by three decimal numeric
components. Missing files, duplicate `version=` entries, surrounding text, and
pre-release/build suffixes are rejected.

## Files Kept In Sync

Each release updates these values before committing:

- `.env`: `version=vX.Y.Z`
- `.env.example`: `version=vX.Y.Z`
- `package.json`: root `version`
- `package-lock.json`: top-level `version` and `packages[""].version`
- `pyproject.toml`: `[project].version`
- `uv.lock`: the local virtual `kivo` package version
- `src-tauri/Cargo.toml`: `[package].version`
- `src-tauri/Cargo.lock`: the local `kivo` package version only
- `src-tauri/tauri.conf.json`: root `version`

Dependency packages that coincidentally have version `0.1.0` are not Kivo
versions and must not be changed. Tests and historical documents containing
version examples are fixtures or evidence and must not be rewritten.

## Version Tool

Add one dependency-free Python tool responsible for parsing, updating, and
checking repository versions. JSON files are read and written through the
standard JSON API. TOML and lock-file updates are constrained to their named
Kivo project/package blocks, and the tool fails unless each expected field is
found exactly once.

The tool exposes three operations:

- Read the tag-form version from `.env`.
- Set every version-owned file to a supplied tag-form version.
- Check that all tracked version fields match `.env` or an explicitly supplied
  release tag.

The same parsing function is reused by PlatformIO when no explicit firmware
build ID is provided.

## Firmware Version Resolution

`Makefile` no longer contains `0.1.0+dev`. It optionally includes `.env`, maps
its lowercase `version` value to `BUILD_ID`, and gives firmware build/upload
targets a prerequisite that reports how to create `.env` when the value is
missing or invalid. A caller may still explicitly override `BUILD_ID`.

`scripts/platformio_build_id.py` resolves the build ID in this order:

1. Use a non-empty `KIVO_FIRMWARE_BUILD_ID` supplied by release CI or another
   explicit caller.
2. Otherwise read the exact tag-form value from the repository-root `.env`.
3. Fail with an actionable message when `.env` is missing or invalid.

No path adds `+dev`. For `.env` containing `version=v0.5.10`, a normal local
firmware build reports `v0.5.10`.

## Release Transaction

`make release` invokes the release script from the repository root. The script
performs these steps in order:

1. Require an existing, valid `.env`.
2. Require a completely clean Git worktree, including staged, unstaged, and
   non-ignored untracked files. Ignored `.env` and build outputs do not make the
   tree dirty.
3. Resolve the target. Without `V`, increment the patch component from `.env`.
   With `V=vX.Y.Z`, use that exact version.
4. Reject an existing local tag before changing any file.
5. Update `.env` and every tracked version file.
6. Run the version consistency check against the target.
7. Stage only the tracked version files listed above.
8. If those files changed, create `chore: release vX.Y.Z`.
9. Create annotated tag `vX.Y.Z` on the resulting `HEAD`, with message
   `release vX.Y.Z`.

The command never pushes. No tag is created when validation, update, staging,
or commit fails. Because all preconditions run before mutation, ordinary input
failures leave the checkout untouched. A failure after mutation leaves the
version changes visible for diagnosis rather than deleting files implicitly.

If all tracked versions already match an explicitly requested target and the
tag does not exist, no empty commit is created; the annotated tag is placed on
the current clean `HEAD`.

## CI Contract

Tagged release CI stops rewriting `src-tauri/tauri.conf.json`. It runs the
version consistency checker against `GITHUB_REF_NAME` and fails if the tag and
committed repository versions differ. Release firmware continues to receive
the explicit tag through `KIVO_FIRMWARE_BUILD_ID`.

Windows CI creates its ignored `.env` from `.env.example` before invoking
PlatformIO native tests. This exercises the same first-checkout setup described
for contributors without committing a machine-local `.env`.

## Contributor Documentation

The README local-development bootstrap includes this step before dependency
installation or any `make` target:

```bash
cp .env.example .env
```

The surrounding text explains:

- `.env` is intentionally untracked and required by local firmware builds and
  `make release`.
- Its single supported entry is `version=vX.Y.Z`.
- New contributors should copy `.env.example`; they do not need to invent a
  version.
- `make release` updates both files, commits tracked version metadata, and tags
  only after that commit succeeds.

The PowerShell instructions provide the equivalent command:

```powershell
Copy-Item .env.example .env
```

## Failure Behavior

- Missing `.env`: fail with the exact creation command appropriate for the
  current shell documentation.
- Dirty worktree: list the dirty paths and exit before version mutation.
- Invalid or inconsistent version fields: identify the file and expected value
  and exit without tagging.
- Existing tag: exit before version mutation.
- Git commit failure: retain the explicit version changes, do not tag.
- Git tag failure: retain the release commit, report that no tag was created.

## Verification

Release tests use temporary Git repositories and real commits/tags. They cover:

- patch bump updates all authoritative and derived version fields;
- explicit `V=vX.Y.Z` uses the requested version;
- the release commit contains only tracked version files;
- the annotated tag points to the release commit, never its parent;
- staged, unstaged, and untracked dirty states are each rejected without
  mutation, commit, or tag;
- missing/invalid `.env` and existing tags are rejected before mutation;
- already synchronized explicit versions create no empty commit;
- Kivo lock-file entries change while unrelated `0.1.0` dependencies do not;
- Make and direct PlatformIO builds resolve the exact `.env` tag without
  `+dev`;
- README onboarding and CI initialization remain present;
- release CI checks the committed version instead of rewriting it.

After the focused release tests pass, run the repository's complete `make test`
gate and `git diff --check` before committing the implementation.
