# Working on this repo (read by every Claude session)

The user starts a new chat often, so everything a session needs to know is
here or in `docs/CLAUDE_HANDOFF.md`. Nothing is remembered between chats.

## Session start

1. If `/home/user/p8work`, `/home/user/p8x` or `/home/user/p8data` are
   missing: `sh /home/user/project-8-data/backup/restore.sh`.
2. Read `docs/CLAUDE_HANDOFF.md` (the entries the user names) and the
   `research/NOTES.md` sections they name in the private data repo.
3. To build or check `p8-game` in the cloud (it needs system libraries):
   `apt-get install -y libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev pkg-config`,
   then `cargo check -p p8-game`. It still cannot be run here; the user tests it.

## The user's rules

`AGENTS.md` applies too, except its `cargo test --workspace`: the rule
below (changed crates only) wins.

- Translate the retail code; don't guess. Cite the function address. Label
  anything unproven INFERRED, APPROXIMATE or UNKNOWN.
- When the user reports a problem, find the cause in the data before
  changing anything.
- **Known issues go in `docs/KNOWN_ISSUES.md`, not fixed.** Any problem you
  find or the user reports that is not part of the current task: add an entry
  there (what the user sees, cause with addresses if known, where in the
  code, CONFIRMED / INFERRED / UNKNOWN). Only fix it if the user asks.
- No game content in this public repo (the approved README screenshots are
  the only exception).
- Plain English, few questions, double-click launchers for anything the
  user runs.
- **Two platforms, always.** The user plays on Windows and macOS. Every
  launcher exists twice, a `.bat` (CRLF) and a matching `.command` (LF,
  executable, `#!/bin/bash` that must work in macOS's bash 3.2, `cd
  "$(dirname "$0")"` first, cargo from `~/.cargo/bin`, pauses on errors),
  with the same behaviour and messages. The pairs today: `INSTALL`,
  `BUILD`, `SETUP`, `PLAY`, `TEST` (was `CAMERA_TEST`) and `INSPECT`. When
  you add or change one, change the other in the same commit; commit new
  `.command` files with `git update-index --chmod=+x`; lint them with
  `bash -n` and `shellcheck`. `.gitattributes` keeps the line endings.
  The test checklist lives only in `TEST_CHECKLIST.txt`, which both `TEST`
  launchers print: edit that file for each new test build, never the
  launchers. Code changes must build on both (no platform-only APIs
  without a `cfg` for the other); the cloud can only check Linux, so
  review platform differences by hand and say which platform the user
  should test on. Downloads: every push to `main` runs
  `.github/workflows/downloads.yml`, which checks that the game compiles on
  Windows and macOS and then publishes `Project8-Windows.zip` (no
  `.command`) and `Project8-macOS.zip` (no `.bat`) to the `latest` release,
  made by `scripts/package.sh`. After pushing, check that run went green
  and tell the user. New launchers must end in `.bat` / `.command` so they
  land in the right zip. Known difference: no controller rumble on macOS (gilrs
  has no force feedback there; it fails silently).
- Save usage: read only the parts of files you need; test only the crates
  you changed (never `cargo test --workspace`).
- One task per session. At the end: run the tests, commit and push, and
  update NOTES, CHECKLIST and the handoff.

## Before calling a feature done (every session)

These exist because the user found bugs that both of us had missed. Run them
and tell the user the results in the final summary.

1. **Branch audit** for every retail function you translated or changed:

       python3 /home/user/project-8-data/tools/branch_audit.py <addr> crates/<crate>/src

   It lists the conditional branches and calls of the retail function that
   the Rust does not cite. For each one, either translate it and cite the
   address, or mark it in a comment with the address and "not translated".
   Report "N of M uncited" to the user. Cite a range `A..B` only for code you
   really translated.
2. **Known-bug sweep**: check the change against each shape below.
3. **Headless run**: an example that prints the new behaviour per frame
   (like `pivot_check`, `spacewalk_check`, `camera_check`). If the user sent
   a clip, compare the printed values with what the clip shows. Re-run the
   older examples the change could affect and compare before/after output.
4. If `p8-game` changed: `cargo check -p p8-game` (see Session start 3), then
   read the diff again line by line.

## Known bug shapes (past bugs; look for the same shape elsewhere)

- **Name to global struct** (NOTES 38): a script param that names a global
  struct must be followed to the global first, then the script's locals
  (`StructureContains` 822ACAD0, getter 82212218).
- **`id` + `sync` tags** (NOTES 37, 40): an anim node built with `id` and
  `sync` starts from the object tag named by its id, and its update writes
  the tag. Done for skatertimer, modulate, takeoffblend, ollielandblend.
  `speedtimerthreeway` (walking) also has it and is not translated. Any
  new node type: check its init for "Id" / "sync" reads.
- **Missing data defaults** (NOTES 35): retail's value for a missing item
  (e.g. a clip's duration is 1.0, not 0) must be read, not assumed.
- **Two copies of one state** (NOTES 36, 39): retail often keeps a
  component copy and an object copy (display matrix `+32` vs object matrix;
  camera `+64` vs camera object). Check which copy each read uses.
- **Untranslated command answers false** (NOTES 43): the script VM returns
  false for a command it does not know (`vm.rs`), so `if not <cmd>` silently
  takes one branch every time (the revert always went frontside because
  `LastSpinWas` was missing). When behaviour never varies, run with
  `P8_TRACE=1` and check the skater's `untranslated` list for the commands
  on that path.
- **Flags taken as "clear"**: search the code for "taken as clear" and
  UNKNOWN when a symptom has no obvious cause.
- **A script check fails for no clear reason**: suspect a name lookup gap
  first (see the first shape).
