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
  (The grind trick lists work since this session, `grind_ride`.)
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

### 11. Grinds: the camera acts as on the ground

- **What you see**: while grinding, the camera follows as if you were
  riding on the ground: no grind zoom, no swing when the grind starts, no
  tilt with the balance.
- **Cause**: the skater camera's grind parts (state 4 in `820D1238`: the
  grind-start slerp, the lean roll `+340`, and the grind zoom in
  `820D02A8` at 820D036C..820D0384, which also depends on a local flag of
  `820D1238` (`stack +160`) whose meaning is not read) are not translated.
- **Where**: `crates/p8-skater/src/camera.rs` (`zoom_and_above`,
  `update`).
- **Label**: CONFIRMED (missing translation).

### 12. Ollie out of a lip jumps back to the grab point first

- **What you see**: in traces, the position goes back to where the lip was
  grabbed (`+1248`) before moving on.
- **Cause**: this is what retail does. Listed only because it looks wrong in
  traces.
- **Where**: `lip.rs`.
- **Label**: CONFIRMED (not a bug).

### 13. Grinds: no sparks, no sounds, no score

- **What you see**: grinding is silent, with no sparks, and gives no points.
- **Cause**: there is no sound system yet (`82115B58` grind sounds,
  `SetRailSound` `820D5640`, `spawnterrainsound`), sparks
  (`SetSparksPos`, `TurnSparksOn`) and scoring (`8217ACB0` per frame,
  `+2220` grind tweak, the "robot rail" part of `82190C10`) are not
  translated.
- **Where**: `grind.rs` (`rail_tail`, `stall_update` notes), `balance.rs`
  (`start`).
- **Label**: CONFIRMED (missing translation).

### 14. Falling off a grind meter leads into untranslated bails

- **What you see**: when the grind meter tips over, the skater drops off
  the side and then behaves oddly (it may stop dead or skid).
- **Cause**: `SkateInOrBail` goes to `SkateIn_Left` / `SkateIn_Right` or
  the grind bail (`FiftyFiftyFall` etc.); bails are not translated.
- **Where**: scripts; the bail system (planned after grinds and the flip
  mechanic).
- **Label**: CONFIRMED.

### 15. Rails on moving objects, rail TriggerScripts, created parks

- **What you see**: nothing yet on z_houses; rails that move with an
  object, and level scripts that run when a rail is taken or left, do
  nothing.
- **Cause**: not translated: moving contacts (820F8DDC..820F9050,
  820F4EC0..820F5148), the rails of moving objects in the searches
  (820F617C..820F68B4), the rail node TriggerScripts (`820F0550`, types
  264, 8200, 8208, 20, 18, 0x1008, 0x20008, 0x20010), and the
  created-park branches (`82194D20`, 820FA3A8..820FA4CC, 820DCC94..).
- **Where**: `grind.rs`, `lip.rs`.
- **Label**: CONFIRMED (missing translation).

### 16. Rail search: the same-object preference is missing

- **What you see**: at the end of a rail, with two other rails almost
  equally close, the grind may carry on to a different one than the
  original picks.
- **Cause**: `821968F8` doubles the score of rails on another collision
  object than the current rail (`82194940`, 82196EE0..82196F10); our level
  collision has no object ids.
- **Where**: `crates/p8-skater/src/rails.rs` (`search`).
- **Label**: INFERRED (the effect; the code is CONFIRMED).

### 17. The stall (RT across a rail) is untested

- **What you see**: unknown; not tried in a headless run.
- **Cause**: `820F4DE8` is translated, but no test run has entered it
  yet.
- **Where**: `grind.rs` (`stall_update`).
- **Label**: UNKNOWN.

### 18. Script expressions call commands that retail may not

- **What you see**: nothing known yet.
- **Cause**: in an expression `( ... )`, retail calls only C functions
  (symbol type 8, table at 8220BCE0); member functions (type 9, most
  skater commands) go to 8220BEA4, which was not read. Our VM calls every
  command it knows there. Scripts were fixed this session (a script name
  in an expression stays a name; that broke the `grind` script).
- **Where**: `crates/p8-script/src/vm.rs` (`expression`).
- **Label**: UNKNOWN.

### 19. Branch audit leftovers

- `8220B878` (script expressions) 70 of 76 and `820D71B0` (`SetState`) 28
  of 38 branches uncited: older partial translations, only small parts
  changed this session.
- clippy 1.97 warns in `anim_tree.rs`, `p8-formats/src/scene.rs` and the
  `pivot_check` example (code not touched this session).
- **Label**: CONFIRMED.

### 20. Documentation leftovers

- `CHECKLIST.md` section 2 counts are stale (handoff section 4).
- `crates/p8-skater/src/lib.rs` refers to `docs/translation.md`, which does
  not exist.
- **Label**: CONFIRMED.
