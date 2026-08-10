# Adaptive Codex Display Final Fixes Report

Base: `1c942b4668fb091f8b3ba9a84d05a857a3152835`

## Fixes

- Terminal rollout completion and interruption now clear every outstanding
  `request_user_input` call for the same rollout task. Runtime, bounded initial
  scan, and persisted-cursor restoration no longer leave `NEEDS INPUT` active
  after the turn is terminal.
- Rollout health now requires a readable sessions directory and successful
  continuation of every tracked file cursor. A missing file is treated as an
  expected deletion only after root enumeration and that file's parent directory
  are readable. A root or parent-tree outage retains in-memory and persisted
  cursors; directory enumeration, permission, file read, rewrite recovery,
  complete-record UTF-8, and JSON failures make the rollout channel unavailable.
  Incomplete final records remain pending.
- One absolute one-second App Server response deadline is created at metadata
  poll entry and shared by initialization and every `thread/list` page. A later
  page receives only the remaining budget. Timeout, EOF, response error, or
  malformed response disconnects the client before any partial page set or
  watermark is committed. Disconnect still kills/reaps the child and joins its
  stdout reader; it does not refresh the deadline per page.
- The newest waiting task remains selected by updated time and stable ID tie
  break. Other waiting tasks render as a bounded `+N` in `row0_right`; long
  labels yield right-side space to the indicator, and counts above 999 display
  `+999+`. The original 8+8 label split remains unchanged with one wait.
- Strengthened the dirty-tile OR/coalescing test with exact disjoint, overlapping,
  re-marked runs; asserted failed display replay stops and joins its worker once;
  and extended complete semantic equality coverage to metric, progress, equal
  expiry, and a one-nanosecond expiry difference.

## TDD Evidence

- Terminal lifecycle RED: 3 expected failures; completion, interruption after
  cursor restore, and initial scan all retained `Some(UserInput)`. GREEN:
  `display::codex_events` 7/7.
- Rollout health RED: mixed good/corrupt cursors and an unreadable sessions
  directory both returned `Ok(Degraded)`; expected deletion already passed.
  GREEN: all three discriminating health/deletion tests passed. A follow-up RED
  showed a malformed complete append also returned `Ok(Degraded)`; GREEN keeps
  it unavailable while the existing incomplete-tail test passes.
- Metadata deadline RED: two compile errors for the missing absolute-deadline
  response seam. GREEN: the first page succeeds, 180 ms is consumed from a
  250 ms test budget, and the second page times out within the remaining budget;
  the full Codex source suite passed.
- Multi-wait RED: 4 expected failures; short/tied/large cases had an empty right
  region and the long label consumed all eight characters. GREEN: renderer
  tests 16/16.
- Final focused verification: 88 display tests passed; the replay stop/join test
  passed; native firmware tests passed 89/89; rustfmt and Clippy passed.

## Fresh Automated Gate

Exact command:

```text
rtk env PATH=/Users/leo/work/kivo/.superpowers/sdd/bin:/Users/leo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin:$PATH make test
```

Result: PASS.

- Python suites: 33 + 32 + 32 passed.
- PlatformIO native: 89/89 passed.
- Rust: 378 passed, 1 intentional ignore, plus 2 integration tests passed.
- Clippy: `--all-targets -- -D warnings` passed.
- Frontend: 211/211 passed; production build passed.
- RP2040 build: PASS; 22,912/262,144 bytes RAM (8.7%),
  133,176/16,773,120 bytes flash (0.8%).
- ESP32-S3 build: PASS; 35,644/327,680 bytes RAM (10.9%),
  348,169/3,342,336 bytes flash (10.4%).
- `git diff --check`: PASS.

## Notification Admission And Authority Follow-up

The final watcher-authority fix was implemented on top of `d942b35` without a
recurring discovery scan.

- Admission RED: directory create and rename paths with `.jsonl` names entered
  the pending set, and a non-JSONL file was passed to `sync_file`. More than 256
  lexical aliases for one deleted rollout occupied all pending slots and set the
  overflow bit. A `RenameMode::From` directory path arriving after the rename
  also entered pending because its old path no longer had directory metadata.
- Admission GREEN: paths are lexically normalized and canonicalized when
  possible before pending capacity accounting. Only `.jsonl` file candidates
  inside the sessions root are admitted. Existing directories, explicit folder
  create/remove events, directory rename pairs, missing rename sources, and
  non-JSONL paths are ignored. Missing `.jsonl` paths from file-removal events
  remain eligible for the confirmed-readable deletion flow. The 257 aliases now
  occupy one pending entry without overflow.
- Authority RED: notify channel errors and pathless `need_rescan` events both
  returned `Ok(Degraded)` while rollout health remained usable. The watcher
  registration seam and authority state were initially absent; after adding the
  minimal seam, a failed registration followed by successful discovery still
  left authority clear and rollout health Healthy.
- Authority GREEN: notify errors and rescan signals immediately set sticky
  `notify_authority_lost` and rollout Unavailable. Ordinary interval polls,
  readable-root checks, pending sync, and successful known-file stats cannot
  clear it. Configuration starts pessimistically and clears authority, overflow,
  and rollout unavailability only after both watcher registration and the
  authoritative startup discovery succeed. Failed registration and failed
  discovery each retain authority loss and overflow.
- Health-transition self-review covered construction, successful and failed
  configuration, channel error, rescan, overflow, root outage, non-due pending
  sync, due pending/tracked sync, and successful reconfiguration. No unrelated
  success path can promote rollout health while authority or overflow is lost.
  Runtime interval polling continues to stat only pending and tracked paths.
- Focused `display::codex`: 53 passed. Rustfmt and Clippy with all targets and
  features passed.

Fresh authority follow-up gate results:

- Exact pinned `make test`: PASS.
- Python suites: 33 + 32 + 32 passed.
- PlatformIO native: 89/89 passed.
- Rust: 390 passed, 1 intentional ignore, plus 2 integration tests passed.
- Frontend: 211/211 passed; production build passed.
- RP2040 build: PASS; 22,912/262,144 bytes RAM (8.7%),
  133,176/16,773,120 bytes flash (0.8%).
- ESP32-S3 build: PASS; 35,644/327,680 bytes RAM (10.9%),
  348,169/3,342,336 bytes flash (10.4%).
- `git diff --check`: PASS.

## Privacy And Diff Review

- No new log event or dynamic log field was added. Existing display status logs
  remain limited to provider ID, health, a static error code, and item count.
- Rollout parsing still projects only lifecycle/session fields. Tests verify
  message content is ignored and cursor persistence contains no body content.
- Metadata pagination remains `thread/list` with `useStateDbOnly: true`; no
  start, resume, mutation, or conversation-body request was added.
- Identical complete semantic snapshots, including identical expiry, remain
  deduplicated; metric, progress, expiry, and task membership changes emit once.

## Remaining Physical Gaps

Not run: firmware upload, attached OLED visual inspection, USB reconnect test,
logic-analyzer/I2C timing capture, and sustained physical key-scan latency test.
The automated gate and firmware builds prove host logic, native firmware logic,
compilation, and linking, not physical acceptance of the display or input timing.

## Final Re-review Follow-up

The follow-up was implemented on top of `17a26ce` without adding a recursive
runtime discovery walk.

- RED: renaming the complete sessions root away made rollout health unavailable,
  but the poll also reduced the tracked cursor set from 1 to 0. This proved that
  child `NotFound` handling erased recovery state and the next persistence pass
  could replace the cursor store with an empty set.
- GREEN: the one-second health cycle enumerates only the sessions root. It returns
  before notify consumption, child sync, or cursor persistence when that fails.
  A tracked child `NotFound` is deletion only when the cycle has confirmed the
  root and the child's immediate parent directory is readable. Non-interval
  notify sync can advance existing files but cannot delete missing cursors.
- The rename-away regression proves unavailable health during the outage,
  byte-identical persisted cursor retention, restoration of the same running and
  user-input state, and continued parsing of a later `function_call_output`.
- The individual-file deletion, mixed corrupt cursor, corrupt complete append,
  unreadable root, and `runtime_stat_poll_updates_known_files_without_recursive_discovery`
  regressions remain GREEN.
- Focused `display::codex` verification: 43 passed. Rustfmt and Clippy passed.

Fresh follow-up gate results:

- Exact pinned `make test`: PASS.
- Python suites: 33 + 32 + 32 passed.
- PlatformIO native: 89/89 passed.
- Rust: 380 passed, 1 intentional ignore, plus 2 integration tests passed.
- Frontend: 211/211 passed; production build passed.
- RP2040 and ESP32-S3 firmware builds passed with the same size figures above.

## Pending Notification Follow-up

The final notification retry fix was implemented on top of `b445c27` without
adding recursive runtime discovery.

- RED: a newly notified rollout containing valid session/task records followed
  by a malformed complete record was drained after its first failed sync. At the
  next due health poll the source incorrectly returned `Ok(Degraded)` with no
  tasks instead of `codex_channels_unavailable`; correcting the file without a
  second notification could therefore never recover it.
- GREEN: notify paths enter a bounded 256-path pending set. Failed syncs remain
  pending, and every pending result contributes to the next due health check.
  Paths leave the set only after a successful sync or an expected deletion whose
  sessions root and immediate parent are confirmed readable. Correcting the
  malformed file without another notification recovers its task, cursor, and
  `NeedsInput` state on the following health cycle.
- Queue overflow is sticky and keeps rollout health unavailable until a
  successful authoritative rollout-home configuration/discovery. The 257th-path
  regression proves a dropped overflow notification cannot be hidden by a later
  Healthy result. Periodic polling still stats only pending and tracked paths;
  the existing no-recursive-discovery regression remains GREEN.
- An actual queued deletion during the sub-second window retains the in-memory
  and byte-identical persisted cursor until the due health check, then removes it
  under the existing readable-root/readable-parent rule. A nested immediate
  parent rename outage retains the cursor and pending path, reports rollout
  unavailable, and restores the same task and input state after the parent is
  restored.
- Focused `display::codex`: 47 passed. Rustfmt and Clippy with all targets and
  features passed.

Fresh final gate results:

- Exact pinned `make test`: PASS.
- Python suites: 33 + 32 + 32 passed.
- PlatformIO native: 89/89 passed.
- Rust: 384 passed, 1 intentional ignore, plus 2 integration tests passed.
- Frontend: 211/211 passed; production build passed.
- RP2040 build: PASS; 22,912/262,144 bytes RAM (8.7%),
  133,176/16,773,120 bytes flash (0.8%).
- ESP32-S3 build: PASS; 35,644/327,680 bytes RAM (10.9%),
  348,169/3,342,336 bytes flash (10.4%).
- `git diff --check`: PASS.
