# Known issues

One place for problems we park until all the mechanics are in. Sessions add
issues here instead of fixing them, unless the user asks for a fix (see
`CLAUDE.md`). When an issue is fixed, delete its entry in the same commit and
say so in the handoff.

Each entry has:

- **What you see**: the symptom as the user sees it.
- **Cause**: with retail addresses, if known.
- **Where**: the file or function in this repo.
- **Label**: CONFIRMED (the cause was proven in the retail code or a trace),
  INFERRED (likely, not proven) or UNKNOWN (cause not looked for yet).

Clips the user sent are named by their upload file name. They are not kept in
this public repo.

---

## Reported by the user

### 1. Handplant goes into a T-pose (holding A, or double-tapping X)

- **What you see**: going into a handplant on a lip while already holding A
  puts the skater in a T-pose, and the trick never starts. Double-tapping X
  on the lip does the same. Clip: `2026-09-30_21-09-10.mp4`.
- **Cause**: not looked for yet. A T-pose usually means the animation tree
  was asked for a clip or branch it does not have (INFERRED).
- **Where**: lip trick entry (`crates/p8-skater/src/lip.rs`, the lip pending
  tricks in `trick.rs`) and the lip branch of `anim_tree.rs` (INFERRED).
- **Label**: UNKNOWN.

### 2. Vert reverts may use the wrong animations

- **What you see**: the revert after landing a vert air in a bowl looks like
  the wrong animation. Clip: `2026-09-30_21-11-43.mp4`.
- **Cause**: not checked yet. The revert is picked by the script `revert`
  (GroundTricks) through `LastSpinWas` (`820D5908`). The vert landing may
  need a different revert list or a different spin direction (INFERRED).
- **Where**: `crates/p8-skater/src/skater.rs` (revert, near the
  `lastspinwas` comment), `core_physics.rs` (`LastSpinWas` field).
- **Label**: UNKNOWN. The user is fairly sure; check it against the scripts
  when vert is next worked on.

### 3. Double-tap X tricks do nothing in manuals

- **What you see**: in a manual, double-tapping X does not do the trick.
  (No clip.)
- **Cause**: the manual's double-tap trick lists are not found in the game
  files or translated yet.
- **Where**: manual trick lists in `crates/p8-skater/src/trick.rs`.
- **Label**: UNKNOWN.

### 4. No comply may not give extra height

- **What you see**: the no comply jumps as high as an ollie. The user thinks
  retail jumps higher. (No clip.)
- **Cause**: what the files say so far is that retail does the same. The
  `nocomply` script (GroundTricks `Jumptricks`, `PressAndRelease up x 300`)
  calls `jump nocomply`, with no `Speed` or `BonelessHeight` (the `boneless`
  script calls `jump bonelessheight`). In `Jump` `820F0730` the `NoComply`
  flag only sets `+2544` (`LastWasJumpBoneless`, `820F0C2C..820F0C7C`), which
  changes the takeoff animation blend, not the speed (NOTES "Jump 820F0730
  flags"). One thing could make it look lower than an ollie: the jump speed
  grows with how long A was held (`820F0BB8`), and a no comply is a quick
  press and release (INFERRED).
- **Where**: `crates/p8-skater/src/air.rs` (`jump`), `skater.rs` (the
  `Jump` command).
- **Label**: INFERRED not a bug. The script and `Jump` are CONFIRMED. To
  settle it, compare a retail clip of a no comply with an ollie that has the
  same A hold time.

### 5. Skating animation goes stiff after a wall push

- **What you see**: after a wall push, when you hold A again, the skating
  animation is stiff, as if stuck in one pose. Clip:
  `2026-09-30_21-14-11.mp4`.
- **Cause**: not looked for yet. Possibly an animation branch or blend the
  wall push sets that is not cleared when you skate away (INFERRED).
- **Where**: wall push in `crates/p8-skater/src/ground.rs` and
  `core_physics.rs`, and the crouch / skating branch of `anim_tree.rs`
  (INFERRED).
- **Label**: UNKNOWN.

---

## Carried over from the handoff (section 11)

### 6. Every lip trick is the Invert

- **What you see**: whatever you press on a lip, you get the Invert.
- **Cause**: the trick queue for lips was not translated when this was
  written. Check whether the trick system work (handoff 17-tricks) fixed it.
- **Where**: `lip.rs`, `trick.rs`.
- **Label**: CONFIRMED when written; may be stale.

### 7. Falling off the bail side of the lip meter looks undefined

- **What you see**: losing balance on the bail side of a lip does odd things.
- **Cause**: it goes to `LipBail` and then the bail scripts, and bails are
  not translated.
- **Where**: `lip.rs` (`LipBail`), `balance.rs`.
- **Label**: CONFIRMED.

### 8. Holding Up does not keep pushing

- **What you see**: holding Up pushes once (6 m/s when leaving the stopped
  state) and never again.
- **Cause**: the repeated push is driven by the push animation, which is not
  in yet.
- **Where**: `core_physics.rs` (push).
- **Label**: CONFIRMED (expected until animations).

### 9. Sideways scraping along walls on the ground can clip

- **What you see**: sliding sideways along a wall on the ground can go
  through it.
- **Cause**: ground side collision `820EB9A0` is not translated. The clipping
  the user saw was on level objects (water, destructible fence), which are
  issue 10.
- **Where**: ground collision in `core_physics.rs`.
- **Label**: CONFIRMED (missing translation).

### 10. Level objects are missing

- **What you see**: water, destructible fences and other objects are not
  there, and you pass through where they should be.
- **Cause**: `LevelObject`, `GameObject`, destructibles and water are not
  loaded; only the static `.hkc` collision is.
- **Where**: level loading (`p8-game`, `p8-sim`).
- **Label**: CONFIRMED.

### 11. Rails that are not lips do nothing

- **What you see**: you can't grind.
- **Cause**: grinds are not translated (`820F8120` rest, `820F4DE8`,
  `820F8CF0`, `820F4108`).
- **Where**: `rails.rs`, `lip.rs`.
- **Label**: CONFIRMED. This session's task.

### 12. Ollie out of a lip jumps back to the grab point first

- **What you see**: in traces, the position goes back to where the lip was
  grabbed (`+1248`) before moving on.
- **Cause**: this is what retail does. Listed only because it looks wrong in
  traces.
- **Where**: `lip.rs`.
- **Label**: CONFIRMED (not a bug).

### 13. Documentation leftovers

- `CHECKLIST.md` section 2 counts are stale (handoff section 4).
- `crates/p8-skater/src/lib.rs` refers to `docs/translation.md`, which does
  not exist.
- **Label**: CONFIRMED.
