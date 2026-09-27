# Handoff: Project 8 Rust/Bevy port

Written by the Claude session that did most of the translation work so far,
for another coding agent who does not have that conversation. It records
what exists, how sure we are of each piece, and what comes next. **Verify
anything important yourself**; addresses and file paths are given so you can.

Certainty labels used below:

- **CONFIRMED FROM ORIGINAL DATA**: read in the retail executable's code or in
  the game's own script/data files.
- **INFERRED**: a reasoned conclusion from original data, not directly read
  (the code comments call this `LIKELY`).
- **APPROXIMATE**: deliberately stood in for, because the original is not read
  or not translated yet.
- **UNKNOWN**: we do not know.

Code comments use `CONFIRMED` / `LIKELY` / `UNKNOWN` with the same meanings as
the first, second and fourth labels.

State as of commit `33b0f11` (engine repo, branch `main`).

---

## 1. Overall goal

Port **Tony Hawk's Project 8 (Xbox 360, 2006)** gameplay to a Rust + Bevy
engine by **faithfully translating the original code**, not by imitating it.
The source of truth is the user's own legally obtained disc, decompiled with
Project8Recomp / rexglue (PowerPC → C++ with the original disassembly kept as
comments). Game data (scripts, levels, animations) is read **at runtime from
the player's own installed copy**. No game content is committed.

The user's standing rules (they are not a programmer):

- "Guess as little as possible." Only implement what the original data shows.
  Do not invent features or behaviour.
- When the user reports a discrepancy, first confirm you understand it and
  look for it in the data **before** changing code.
- Plain English, few questions, double-clickable `.bat` files on Windows.
- Never commit game content (assets, the `.xex`, generated recompiled code,
  extracted files) to the public repo. See `AGENTS.md`.

## 2. Repositories and machine layout

| What | Where | Public? |
|---|---|---|
| Engine (this repo) | `github.com/flevz/project-8-rust-engine`, local `/home/user/project-8-rust-engine` | Intended public. No game content. |
| Private research | `github.com/flevz/project-8-data`, local `/home/user/project-8-data` | **Private. Never copy its contents here.** Holds the user's `default.xex`, `dbg.pak.xen`, `research/NOTES.md` (detailed findings with addresses), `research/CHECKLIST.md` (coverage checklist), `research/tools/*.py`. |
| Research working dir (cloud machine only) | `/home/user/p8work` | Not in git. `image.bin` (loaded image dump), `generated/default/*.cpp` (recompiled code with disassembly comments), `qb_globals.txt`, `strings.tsv`, `script_commands.tsv`, `checksums.pkl`, `extra_names.txt` (118,762 checksum names from `dbg.pak.xen`), `tools/`. |
| User's game data (cloud machine only) | `/home/user/p8data/DATA/...` (e.g. `DATA/COMPRESSED/PAK/qb.pak.xen`, `DATA/COMPRESSED/ZONES/z_houses.pak.xen`, `.../PAK/perm_anims.pak.xen`) | Not in git. |
| Scratch clone, **do not commit here** | `/home/user/sk8-engine-project-8-thing` (a different project that happened to be the session's working dir) | n/a |

The cloud machine is ephemeral; the private repo is the durable copy of the
research. If `/home/user/p8work` is gone, rebuild it using
`project-8-data/README.md` (rexglue codegen with the image-dump patch).

## 3. Current architecture

Cargo workspace (`Cargo.toml`), edition 2024, Bevy 0.18.1, glam 0.30.

| Crate | Role | Status |
|---|---|---|
| `crates/p8-formats` | Readers for the game's files: `.pak.xen`/`.pab.xen` archives (`pak.rs`), QB scripts and globals incl. LZSS (`qb.rs`), QB checksum (`checksum.rs`), Havok level collision (`havok.rs`), zones: collision + restart nodes + rail nodes (`zone.rs`). Bin `p8-inspect`. | Active |
| `crates/p8-script` | The QB **script VM**, translated from the retail CScript code: tokens (`code.rs`), parameter lists (`params.rs`), interpreter (`vm.rs`) behind a `Host` trait. | Active |
| `crates/p8-skater` | The **translated skater**: physics (`core_physics.rs`, `ground.rs`, `air.rs`, `vert.rs`, `lip.rs`), rails (`rails.rs`), balance meter (`balance.rs`), controller path (`controller.rs`, `pad.rs`, `input.rs`), stats (`stats.rs`), script globals access (`script.rs`), level feelers (`world.rs`), and the script host that runs the skater's scripts on the physics (`skater.rs`). | Active, main work |
| `crates/p8-game` | The Bevy app. `translated.rs` = play mode for the translated skater with the player's scripts and level. `main.rs` falls back to the old prototype if the scripts are not set up. Bin `p8-setup` (reads the player's install, writes `scripts-location.txt` and `tuning.json`). | Active |
| `crates/p8-sim` | **Old prototype** simulation (not a translation; approximations with hand-tuned values). Only used when `scripts-location.txt` is missing. | Legacy; do not extend |

Frame flow (translated mode), 60 Hz `FixedUpdate`:

1. `translated.rs::read_pad` builds an Xbox 360 pad state from Bevy input.
2. `controller.rs` converts it the way the retail XInput path does
   (`823A6420` → `8222A320` → `822D6C98`) into an `InputState` (per-button
   held flags, press/release times, stick values).
3. `Skater::step` (`skater.rs`): `CorePhysics::step` (`air.rs`), then, if the
   physics asked for it, a script goto + update (lip trick), then physics
   events are delivered to the skater's running script, then the script is
   updated once.
4. `CorePhysics::step` mirrors retail `820FC990`: crouch update, speed limits,
   state update (ground / air / lip), air ollie trigger, rail check.

Retail offsets are kept in field docs (e.g. `+1540`, "SkaterState `+56`") so
every field can be checked against the original. Every translated function
cites its retail address (e.g. `820D9830`).

## 4. The checklist we have been following

There are two lists.

**A. The roadmap the user agreed** (from the conversation; the older
`docs/ROADMAP.md` is stage-based and stale):

1. Riding, ollie, air, landing, ground contact, level loading: **done**.
2. Air pieces (leveling, wall collision, step-up, late ollie): **done**.
3. Vert, quarter pipes and lips: **vert done; lips mostly done** (see §5).
4. Tricks: grinds, manuals, flip/grab tricks with double-tap variants:
   **not started** (only the shared pieces: rails, balance meter).
5. Bails: **not started**.
6. Looks: skater model, animations, camera, pushing, stats menu: **not
   started**. The user decided pushing waits for animations, and the
   sideways-landing issue waits for a stats menu.

**B. The coverage checklist** in the private repo,
`project-8-data/research/CHECKLIST.md`, generated by
`research/tools/physcov.py` and `reach2.py`. It lists every function the
per-frame, ground and air updates call (T/P/N), and every script command the
skater's scripts can reach (452 commands from 595 reachable scripts). Its
section 1 rows for `820E0188`, `820F49D8`, `820DA1A8`, `820DC5D0` were updated
by hand; **its section 2 counts are stale** (they predate the vert and lip
commands). Regenerate it before relying on the counts.

## 5. What is implemented, by area

### Complete (translated, tested on the real level)

| Area | Retail functions | Files |
|---|---|---|
| Ground update: gravity along the ground, kick flag, braking, kicking/accelerating (crouched autokick), friction (rolling, special, slope turn), steering, velocity along the board, stopped event | `820F6978` (most), `820D9208`, `820D93F0`, `820D9830`, `820D9AB8`, `820E5DB8`, `820D9F70`, `820D9CE8`, `820ECEE8`, `820DB318`, `820D7C88` | `core_physics.rs` |
| Ground move loop, forward wall collision, wall bounce / wall push / flail, ground snap (incl. vert flag bookkeeping) | `820F7C98`, `820EBD20`, `820E5F40`, `820DB858`, `820DB418`, `820F12C0` | `ground.rs` |
| Ollie trigger, Jump (ground and air paths, vert stats), air gravity, air move, landing | `820D7AB0`, `820F0730`, `820D76C0`, `820F2310` (parts) | `air.rs` |
| Air spin / lean, air leveling, air wall collision, ledge step-up, late ollie window | `820E9620` (partial), `820E4AD0`, `820EF410`, `820E4DB8` | `air.rs` |
| Backwards flip on landing / riding backwards | `820DBAA8` (non-manual branch) | `core_physics.rs` |
| Speed limits and `OverrideLimits` | `820E0188`, `820D5C68` | `core_physics.rs`, `skater.rs` |
| Vert: takeoff, vert air, following the wall below, break-vert over the lip, jump out of vert air, vert landing flags, normal easing, sideways uprighting in normal air | `820DA3D0`, `820F2900..820F3014`, `820EC7B0`, `820DA1A8` (easing part), `820DC5D0`, `820D7648` | `vert.rs` |
| Controller path (XInput → PS2-style pad → input records, dead zones) | `823A6420`, `8222A320`, `82229E58`, `8222A030`, `822D6C98` | `controller.rs`, `pad.rs` |
| Script VM (tokens, if/else, loops, switch, random, calls/returns, goto, exceptions/event handlers, wait, expressions, struct includes) | `8220F8F0`, `8220F210`, `8220A228`, `82208CE8`, `8220EF50`, `8220E258`, `8220EE00`, `82224D78`, `822A6FB8`, `8220B878`, `82204838`, `82211BE0`, `82218210`, ... | `p8-script` |
| Stat-scaled values | `82199D00`, `82199A28` | `script.rs`, `stats.rs` |
| Level collision (Havok boxes, cylinders, capsules, triangle meshes) and feelers | `8221BC60`, `8221B3C0`, `82201558` filter | `havok.rs`, `world.rs` |

### Partially complete

| Area | What works | What is missing |
|---|---|---|
| **Lip tricks** | Rails loaded from the level; rail search; rail grab in the air with Y held; lip entry checks and snapping; lip state; balance meter; ollie out; falling off the meter; the `liptrick` → `InvertTrick` → `LipOut`/`OllieLipOut` scripts run for real | The **trick queue** (`SetQueueTricks`, `DoNextTrick`, button triggers) is not translated, so every lip is the default Invert (`DefaultLipTrick`); bails (`LipBail`); animations; score. |
| Rails | Build (`82197138`, `821939F8`) and search (`821968F8`) for normal levels | Grinds (`820F8120` grind set-up, `820F4DE8`, `820F8CF0`), single-node rails (`820F4108`), moving-object rails, park-editor paths, `CreatedFromVariable`/`createdfromtod` rails |
| Balance meter | Lip meter start/update/stop (`820CF748`, `82190C10`, `82190F58`, `820CEAE8`) | Grind-only parts (same/new rail timing, robot rail), manual use (manuals not translated), display, cheats |
| Air update `820F2310` | See above | Wallride/wallplant (`820EDAA8`, `820E8618`, `820E80D8`), pitch bail (`820F31E4`), high ollie `820D79F8`, spine transfer `820EA0D0`, bikes, moving platforms, nose/tail contact feelers `820E5250` |
| Air spin `820E9620` | Spin and lean | Vert auto-turn, SmoothSpin, Nail the Trick |
| Ground update `820F6978` | See above | **Ground side collision `820EB9A0`**, manuals branch, skitching, high ollie, several animation/bookkeeping calls |
| Script commands | 64 translated in `skater.rs` (list: `COMMANDS` const) plus VM built-ins in `vm.rs` (`is_vm_command`) | The rest remain: animation, sound, trick system, scoring, UI, goals, walking, ragdoll, bikes. Untranslated commands return "not handled" and are listed in the game's HUD. |

### Not started

Grinds, manuals, flip/grab tricks and the trick system (queue, triggers,
double taps, trick names, scoring), bails and ragdoll, wallrides and
wallplants, spine transfers, walking, skitching, special meter, Nail the
Trick, skater model, animation system (anim tree, clips), pushing (depends on
animations), retail camera, textured level rendering, audio, menus, stats menu.

## 6. Important files and what they do

| File | Contents |
|---|---|
| `crates/p8-skater/src/core_physics.rs` | `CorePhysics` (all physics state, with retail offsets), `Event`, `State` (Ground/Air/Lip), `Vert`, ground update, speed limits, friction, steering, backwards flip, tests. |
| `crates/p8-skater/src/air.rs` | `set_state` (part of `820D71B0`), Jump, air update, air leveling, wall collision, landing, and `step()` (the per-frame dispatch). |
| `crates/p8-skater/src/ground.rs` | Ground move loop, forward collision, wall response, wall push, ground snap, `orient_to_ground` (`820D7648`). |
| `crates/p8-skater/src/vert.rs` | Vert takeoff/air/tracking/break-vert, normal easing, uprighting, `rotate_about_row0/at`. |
| `crates/p8-skater/src/rails.rs` | `RailManager::build` and `search`. |
| `crates/p8-skater/src/lip.rs` | Rail check (`820FAAA8`), may-take-rail (`820DCBE8`), grab (`820F8120` up to the lip), lip entry (`820F44C0`), lip update (`820F49D8`), `SkateInAble` (`820E55E0`), `random()`. |
| `crates/p8-skater/src/balance.rs` | Balance component and meters. |
| `crates/p8-skater/src/skater.rs` | `Skater` (physics + running script), the script host `Ctx` with all translated skater commands, event name mapping, `COMMANDS` list. `P8_TRACE=1` prints every command the scripts run. |
| `crates/p8-skater/src/script.rs` | `Scripts`: globals, `physics_float`, `global_float`, `stat`, `stat_value_of`, terrain lookups. |
| `crates/p8-skater/src/controller.rs`, `input.rs`, `pad.rs` | Controller path and `InputState`. |
| `crates/p8-skater/src/world.rs` | `World` trait (`feeler`, `rails`), `FlatFloor`, `Level` (Havok collision + rails). |
| `crates/p8-script/src/vm.rs` | Script VM; tests with a tiny assembler. |
| `crates/p8-script/src/params.rs` | `Params` (retail `CStruct` semantics: AddComponent, lookups, `get_in` with struct includes, `resolve_alias`). |
| `crates/p8-formats/src/zone.rs` | Zone loading: collision, restarts, rail nodes, compressed-node template expansion. |
| `crates/p8-game/src/translated.rs` | Bevy play mode: level mesh from collision, placeholder skater and camera, HUD, input. |
| `crates/p8-skater/examples/*.rs` | Headless test rides on the real level: `level_ride`, `script_ride` (real scripts), `vert_ride` (a halfpipe; `up` holds Up), `lip_ride` (holds Y; `ollie` ollies out). |
| `crates/p8-script/examples/check_scripts.rs` | Walks every script in the player's `qb.pak.xen` (currently 7626, all clean). |
| `project-8-data/research/NOTES.md` (private) | Sections 1–19: every finding with addresses. The detailed source for everything here. |
| `project-8-data/research/tools/` (private) | `fn.py <addr>` (disassembly from generated code), `gen.py <addr>` (with switch cases), `callers.py`, `who.py <regex>` (functions matching an instruction), `qbscript.py <file|all> <regex>` (script printer), `qbdec.py` (QB globals), `pak.py` (extract paks), `toks.py`, `mini.py`, `physcov.py`, `reach2.py`. |

## 7. Architectural decisions and why

1. **Translate, don't approximate.** Each function is read in the retail code
   and translated line by line; behaviour not yet read is **absent**, not
   imitated (`crates/p8-skater/src/lib.rs` header). Reason: the user's
   requirement to guess as little as possible.
2. **Run the real scripts.** Much of the skater's behaviour (ollie, landing,
   stopping, lips) is in QB scripts. Instead of re-implementing script logic,
   a faithful script VM (`p8-script`) runs the player's own scripts, and the
   physics exposes retail script commands. This is why many behaviours "just
   work" once the right commands exist.
3. **No game values in the code.** Tuning values are read from the player's
   `qb.pak.xen` at runtime (`Scripts`). Only literal constants embedded in
   retail code (e.g. `0.0025` at `820027D0`) appear in the source, each cited.
4. **Unknown commands answer "not handled".** The VM treats an unknown symbol
   as TRUE (retail behaviour, `8220F210`), but host commands that are not
   translated return `None`, which the VM treats as not handled; they are
   collected and shown in the HUD so gaps are visible.
5. **Events are delivered after the physics step** (`skater.rs`), and the
   script updates once per frame after that. Retail's exact in-frame order of
   the script update is **INFERRED**, not read.
6. **Lip entry performs a script goto + update immediately** (as retail does
   at `820F4990`) via `CorePhysics::script_goto`, handled in `Skater::step`.
7. **Pushing deferred.** Holding Up does not push repeatedly because retail
   pushing is driven by the kick animation firing `KickBoostEvent`
   (animation-driven; user agreed to wait for animations).
8. **Placeholder camera and visuals** until models/animations are done.

## 8. Original-game data used as evidence

All CONFIRMED FROM ORIGINAL DATA unless noted.

- **Executable code**: the user's `default.xex`, decompiled by rexglue v0.10.0
  into `/home/user/p8work/generated/default/thps_p8_recomp.*.cpp` (C++ with the
  PowerPC disassembly as comments); a loaded-image dump `image.bin` (base
  `0x82000000`) for reading constants. Every translated function cites its
  address.
- **Checksum names**: `dbg.pak.xen` (118,762 names) → `extra_names.txt`.
- **Command registries**: static initializers decoded with `mini.py`
  (24-byte entries: checksum, component, pointer-to-member). The older
  `member_table.pkl` pairing was wrong in places; use the registry decode
  (`project-8-data/research/component_commands.tsv`, 737 entries).
- **Scripts and globals**: the user's `DATA/COMPRESSED/PAK/qb.pak.xen` (+
  `.pab.xen`), 7626 scripts. Skater physics globals are in `4E919A4D.qb`
  (`skater_physics` struct and `physics_*`); lip tricks in `liptricks.qb`;
  trick tables in `07F056D7.qb` (`DefaultTricks`, `DefaultLip`,
  `DefaultFliptricks`), `airtricks.qb`, `GroundTricks.qb`, `tricks.qb`.
- **Levels**: `DATA/COMPRESSED/ZONES/z_houses.pak.xen` (`.hkc` Havok
  collision; `.nqb` node array `Z_Houses_NodeArray` with Restart and 964
  RailNode nodes; most nodes compressed through
  `Z_Houses_NodeArray_compressed_node_*` template structs).
- **Animations** (only read for timing, not used yet): `perm_anims.pak.xen`
  `.ska` files; the float at `.ska +0x28` is **INFERRED** to be the clip
  length. Animation events table `skateranimeventtable` in `76F68C8D.qb`.

## 9. Gameplay constants

### CONFIRMED FROM ORIGINAL DATA: script globals (read at runtime, not hardcoded)

Selected values from the user's `qb.pak.xen`, for reference:

| Global | Value |
|---|---|
| `physics_air_gravity` | -21.6 |
| `physics_ground_gravity` | -9.8 |
| `physics_air_hang_stat` / `physics_vert_hang_stat` | 0.9 / 1.0 |
| `skater_max_speed_stat` / `skater_max_max_speed_stat` | 18 / 38 (stat structs with modifier) |
| `physics_heavy_air_friction` | 0.0004 |
| `physics_standing_acceleration_stat` / `crouching` | 5.0 / 7.5 |
| `skater_max_crouched_kick_speed_stat` | 9.5 |
| `physics_jump_speed_stat` | 7.6 (min stat 7.0..7.6) |
| `physics_vert_jump_speed_stat` | 5.0..6.0 |
| `landing_velocity_factor` | 0.35 |
| `skater_late_jump_slop` | 333 ms |
| `physics_vert_push_out` | 0.075 |
| `skater_vert_push_time` / `skater_vert_active_up_time` / `skater_vert_allow_break_time` | 130 / 250 / 200 ms |
| `physics_break_air_speed_scale` / `up_scale` | 0.75 / 0.75 |
| `skater_break_vert_forward_tilt` | 45 |
| `skater_upright_sideways_speed` | -60 deg/s |
| `normal_lerp_speed` | 0.1 |
| `rail_max_snap` | 1.016 |
| `lipallowangle` / `_override` / `lipplayerhorizontalangle` / `liprampvertangle` | 25 / 60 / 47 / 68.5 |
| `physics_wallplant_disallow_grind_duration` | 200 ms |
| `balancesafebuttonperiod` / `balanceignorebuttonperiod` | 1000 / 0 ms |
| `lipparams` | stat structs (cheese 3000..1000, instable_rate 0.45..0.2, lean_bail_angle 4000, ...) |
| `kick_boost_increase` / `kick_boost_threshold` | 3 / 10 (pushing, not used yet) |

### CONFIRMED FROM ORIGINAL DATA: literal constants in retail code

These appear in the code with their addresses, e.g. skin distance 0.0025
(`820027D0`), landing speed cutoff 0.254² (`82002ADC`), lip look-across
0.76 (`82002AE4`), rail scoring 1.122 (`82004EE0`), balance dead zone 0.2 /
1.25 (`82000D34` / `820029B8`). Search the code for `is the constant at`.

### INFERRED / APPROXIMATE / TEMPORARY

- **Frame time** is a fixed 1/60 s (`dt`); retail's frame length source is not
  read. APPROXIMATE.
- **Random numbers**: retail `821E8508`/`821E8560` generators are not read; a
  plain LCG stands in (VM `Host::random`, `CorePhysics::random`). APPROXIMATE.
  Affects balance meter wobble and script randoms.
- **Stats**: every stat uses `Skater_Default_Stats` (5 in the data) because the
  player's profile/save stats are not read. APPROXIMATE. This is the likely
  cause of the parked "sideways landing" difference (NOTES "Open item").
- `last_wallplant_ms` starts at "long ago" (retail stores 0 and its clock is
  far past it). INFERRED.
- Terrain checksum → index uses the order of `terrain_types` (retail
  `8228E528` uses a hash table INFERRED to be built from that array).
- `.ska +0x28` = clip length: INFERRED.
- `p8-sim` values (`tuning.rs`) are the old prototype's: TEMPORARY, not used
  in translated mode.

## 10. Systems that are currently approximate or stand-ins

- **Held buttons for scripts** (`Held` command, `skater.rs::held`): retail keeps
  per-button held flags updated from input events (`+2664`); we read the
  controller records directly. INFERRED equivalent.
- **Animation waits** (`Skater_WaitAnimFinished`, `timer_wait`): go through the
  untranslated anim tree, so they end immediately. The lip stall's pause comes
  from `init_skip_time` in the data, which is fine, but any wait that only
  depends on animation length is currently zero.
- **Display matrix** `matrix_32` (retail `+32`): copied or left alone as retail
  does, but the display smoothing part of `820DA1A8` is not translated and
  nothing renders from it.
- **Park editor state** (`[82731B2C]+64`) is assumed "OFF" (normal levels),
  which removes the rail clearance feelers `82194D20` and the corner-leave
  angle. INFERRED correct for normal levels.
- **Spine button (RT)** in break-vert: retail first searches for a spine
  (`820E68A8`, not translated); we behave as if none was found (small push
  over the lip). APPROXIMATE.
- **Struct includes for host commands**: the VM's own lookups follow
  unnamed-name includes (retail `82211BE0`), but host commands in `skater.rs`
  read `Params` directly without includes. APPROXIMATE; fine for the
  commands translated so far.
- **GetSkaterVelocity** returns only `vel_x/y/z` (ints, as retail); its
  `Skew_Angle`/`scaled_*` outputs are not translated.

## 11. Known bugs / incorrect behaviour

- **Every lip trick is the Invert**: the trick queue is not translated.
- **Falling off the "bail" side of the lip meter** goes to `LipBail` → bail
  scripts, and bails are not translated; behaviour there is undefined-looking.
- **Holding Up does not push** repeatedly (only the one-time 6 m/s when leaving
  the stopped state). Expected until animations.
- **Ground side collision (`820EB9A0`) is missing**: sideways scraping along
  walls on the ground can clip. The user said the clipping they saw is on
  objects (water, destructible fence) that are separate level objects, not
  loaded yet; those are also missing.
- **Level objects** (`LevelObject`, `GameObject`, destructibles, water) are not
  loaded; only the static `.hkc` collision is.
- **Rails that are not lips do nothing** (grinds not translated).
- After ollieing out of a lip the position is first restored to where the lip
  was grabbed (`+1248`), then moved; this is literal retail behaviour, noted
  here because it looks odd in traces.
- `CHECKLIST.md` section 2 counts are stale (see §4).
- `crates/p8-skater/src/lib.rs` refers to `docs/translation.md`, which does
  not exist. `README.md` and `docs/ROADMAP.md` describe the old prototype
  stage and are out of date. `zone.rs` calls struct includes "LIKELY"; the
  VM reading (`82211BE0`) has since CONFIRMED lookups follow them.
- `NOTES.md` §6 says the LZSS ring starts at 0; that was wrong and is fixed:
  the ring is filled with spaces (0x20). Code (`qb.rs::lzss`) is correct.

## 12. Areas of uncertainty

- The in-frame order of script updates vs physics events (INFERRED).
- Which button record the left stick feeds for Up/Down/Left/Right (the
  `8222A030` stick → D-pad path is translated; whether scripts' `Held` sees it
  the same way is INFERRED).
- Object `+128` as "position at the start of the frame" (INFERRED; used as
  `old_position`, the lip `+1264` source and feeler starts).
- SkaterState flags whose meaning is UNKNOWN: `+40` (wall flail direction),
  `+216`, `+176`, `+120`, `+200`, `+208`, `+192`, `+240`, `+1380`, `+2024`.
- The anim-tree `kicktimer` node (pushing cadence): UNKNOWN, not read.
- Trick queue / button trigger semantics (`Press`, `TripleInOrder`,
  `TapTwiceRelease`, time windows): UNKNOWN until read.
- `820F01E8` state update and several ground/air helpers listed as "unread" in
  the checklist.

## 13. Work attempted and abandoned or replaced

- **`p8-sim` prototype**: hand-tuned approximation, replaced by the
  translation (`p8-skater`). Kept only as a fallback.
- **Late-ollie emulation without scripts** (`CorePhysics::late_ollie` stand-in
  for the `groundgone`/`ollie` scripts): still used only when scripts are not
  running (`scripted == false`); with scripts, the real scripts handle it.
- **Member-table command pairing** (`member_table.pkl`): partly wrong (e.g.
  `NoSpin`/`CanSpin`, `Jump`/`switched` swapped); replaced by decoding the
  registries' static initializers with `mini.py`. Unicorn could not run those
  initializers.
- **LZSS with a zero-filled ring**: broke 233 scripts; fixed (spaces).
- **Assignment `<x> = v` using the local's value as the target**: fixed to use
  the 4 bytes before `=`.
- **Expression `?global` unpacking a struct into params**: fixed to push the
  whole value (so `2 * ?skater_physics.kick_boost_increase` works).

## 14. Hacks and workarounds in the code

- `CorePhysics::scripted` switches between running the real scripts and the
  built-in ollie stand-in (for running without scripts, e.g. tests).
- `Skater::new` falls back to unscripted mode when `skaterinit` is missing.
- Test worlds in unit tests (`Plane`, `Wall`, `FlatFloor`, synthetic
  `Scripts`) use test data, not game values.
- `RailManager::build` skips a link that does not name a rail record (retail
  assumes all links do).
- Unknown host commands return `None` so they are reported rather than
  guessed.
- Examples hardcode `/home/user/p8work/extra_names.txt` for readable names
  (optional; they still run without it).

## 15. Build, run, test

Windows (user): `BUILD.bat` (release build of `p8-game` and `p8-formats`
bins), `SETUP.bat` (drag the installed game folder; writes
`scripts-location.txt` and `tuning.json`, both gitignored), `PLAY.bat`.
`INSPECT.bat` runs `p8-inspect`.

Linux (agent):

```sh
cargo build --workspace
cargo clippy --workspace --all-targets     # must be clean
cargo test -p p8-skater -p p8-script -p p8-formats
cargo test -p p8-game                      # slow first build (Bevy)
P=/home/user/p8data/DATA/COMPRESSED/PAK/qb.pak.xen
cargo run --release -p p8-script --example check_scripts -- $P
cargo run --release -p p8-skater --example script_ride -- $P /home/user/p8work/extra_names.txt
cargo run --release -p p8-skater --example vert_ride -- $P z_houses 12 [up]
cargo run --release -p p8-skater --example lip_ride  -- $P z_houses 12 [ollie]
P8_TRACE=1 cargo run ...   # print every script command
```

Note: `cargo test --workspace` in one go may exceed a 10-minute tool timeout
because of the Bevy test build; run the crates separately.

## 16. Current build status (commit `33b0f11`)

- `cargo clippy --workspace --all-targets`: clean.
- Tests: p8-skater 46, p8-script 9, p8-formats 8, p8-game 1 + 2: all pass.
- `check_scripts`: 7626 scripts walk cleanly.
- Real-level rides: `vert_ride` (up and down a halfpipe, break-vert with Up),
  `lip_ride` (lip grab, ~1.7 s until the meter tips with no input, ollie out
  to ~2 m) behave as described.
- The user built and played up to the vert commit and reported quarter pipes
  feel right; the lip build has not been played by the user yet.

## 17. The exact next item and what I intended to do

**Next: the trick queue** (task "Trick queue: SetQueueTricks, DoNextTrick,
button triggers"). It is what picks the lip trick from direction + Y, and the
same system gives flip tricks, grabs and double-tap variants, which the user
asked for.

Plan (nothing of this is read yet; all of it is to be read first):

1. Read `SetQueueTricks` (`8211F898`), `DoNextTrick` (`82124960`, it calls
   `821230D8`), `ClearTrickQueue` (`8211FA08`), `KillExtraTricks`
   (`8211FED8`), `SetTrickName`/`Score`, and the trick component's per-frame
   update that matches button triggers against the lists (find it from the
   `trick` component registry and `822D6xxx` input event functions).
2. Read the trigger types used in the data: `Press` (e.g. `liptricks`:
   `{Press, upleft, 500}`), `TripleInOrder` (`SpecialLipTricks`),
   `TapTwiceRelease` (boneless), `PressAndRelease` (nocomply), air flips
   `dir,dir + square`.
3. Resolve trick slots (`Lip_TriangleU` → `DefaultLip` → `Trick_Invert`
   struct with `scr`/`params`) through the skater's trick set
   (`DefaultTricks` / `HawkTricks`).
4. Translate, then re-run `lip_ride` with a direction held and check the
   right `Trick_*` script runs.

After that, in the user's order: grinds (rail grab's grind set-up
`820F8120`, grind update `820F4DE8`, rail update `820F8CF0`; the balance
meter's grind parts), manuals (`82190F58` manual meter is shared), then bails.
The user may instead ask for models and animations; pushing depends on them.

## 18. Do NOT change without understanding the consequences

- **Do not replace translated code with approximations** or "improve" it by
  feel. If behaviour seems wrong, find the retail reason first (the user's
  rule). Cite addresses for any change.
- **Do not hardcode game values** (script globals) in the code, and do not
  commit anything from the game or from `/home/user/p8work`,
  `/home/user/p8data` or the private repo.
- **`p8-script` semantics** (`Params::add` removal rules, `get_in` includes,
  unknown-symbol TRUE, `if` on script calls using returned `TRUE`, the
  expression name-resolution rule and its "not after `.`" exception): the
  skater's scripts depend on these exact rules; `check_scripts` and the rides
  catch regressions.
- **Event delivery order** in `Skater::step` and the lip `script_goto`: changing
  them changes which script handles an event in the same frame.
- **`set_state`** restores `lip_pos` when leaving the lip: removing it changes
  lip exits.
- **Retail offsets in docs** are how translations are checked; keep them when
  refactoring.
- **`controller.rs`** mirrors the retail pad path including dead zones and the
  stick → D-pad mapping; trick input will depend on it.
- **Unit test fixtures** (`scripts()` builders) must include any new globals a
  changed function reads, or a missing global reads as 0 (e.g. a missing
  `skater_max_max_speed_stat` would cap speed at 0).
