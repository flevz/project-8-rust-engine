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

State as of the commit "Show the balance meter on screen (lips)"
(engine repo, branch `main`).

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
- Plain English, few questions, double-clickable launchers: `.bat` on
  Windows and a matching `.command` on macOS.
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
| `crates/p8-formats` | Readers for the game's files: `.pak.xen`/`.pab.xen` archives (`pak.rs`), QB scripts and globals incl. LZSS (`qb.rs`), QB checksum (`checksum.rs`), Havok level collision (`havok.rs`), zones: collision + restart nodes + rail nodes (`zone.rs`), `.img` textures (DXT5 only, `texture.rs`). Bin `p8-inspect`. | Active |
| `crates/p8-script` | The QB **script VM**, translated from the retail CScript code: tokens (`code.rs`), parameter lists (`params.rs`), interpreter (`vm.rs`) behind a `Host` trait. | Active |
| `crates/p8-skater` | The **translated skater**: physics (`core_physics.rs`, `ground.rs`, `air.rs`, `vert.rs`, `lip.rs`), rails (`rails.rs`), balance meter (`balance.rs`) and its on-screen state (`meter_display.rs`), controller path (`controller.rs`, `pad.rs`, `input.rs`), stats (`stats.rs`), script globals access (`script.rs`), level feelers (`world.rs`), and the script host that runs the skater's scripts on the physics (`skater.rs`). | Active, main work |
| `crates/p8-game` | The Bevy app. `translated.rs` = play mode for the translated skater with the player's scripts and level; `balance_meter.rs` draws the balance meter with the game's own HUD textures. `main.rs` falls back to the old prototype if the scripts are not set up. Bin `p8-setup` (reads the player's install, writes `scripts-location.txt` and `tuning.json`). | Active |
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

**A. The roadmap the user agreed** (from the conversation; the old
stage-based `docs/ROADMAP.md` was removed at the user's request):

1. Riding, ollie, air, landing, ground contact, level loading: **done**.
2. Air pieces (leveling, wall collision, step-up, late ollie): **done**.
3. Vert, quarter pipes and lips: **vert done, spine transfers / acid drops /
   bank drops done; lips mostly done** (see §5).
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
| Spine transfer (search over the coping `820E0AC8`, launch, retry `+1380`), acid drop / bank drop (search `820E1600` skating path, start `820DA7B0`), orientation blend `820EA0D0`, SkaterState `+136` effects (vert hang gravity, carry `+272` in the air move, no speed limits / leveling / uprighting, `SetSkaterVelocity` refused, wall projection keeping length), landing redirect along `+2448` with `Physics_Acid_Drop_Min_Land_Speed`, `LandedFromSpine`/`LandedOnBank`, post-transfer speed allowance `820DAFF0`, `+200` "no acid drop" bookkeeping | `820E68A8`, `820E0AC8`, `820E1600`, `820DA7B0`, `820EA0D0`, `820DAFF0`, `820D77F0`, `820F2310` (parts), `820EC7B0` (spine branch) | `transfer.rs`, `air.rs`, `vert.rs`, `ground.rs` |
| Controller path (XInput → PS2-style pad → input records, dead zones) | `823A6420`, `8222A320`, `82229E58`, `8222A030`, `822D6C98` | `controller.rs`, `pad.rs` |
| Script VM (tokens, if/else, loops, switch, random, calls/returns, goto, exceptions/event handlers, wait, expressions, struct includes) | `8220F8F0`, `8220F210`, `8220A228`, `82208CE8`, `8220EF50`, `8220E258`, `8220EE00`, `82224D78`, `822A6FB8`, `8220B878`, `82204838`, `82211BE0`, `82218210`, ... | `p8-script` |
| Stat-scaled values | `82199D00`, `82199A28` | `script.rs`, `stats.rs` |
| Level collision (Havok boxes, cylinders, capsules, triangle meshes) and feelers | `8221BC60`, `8221B3C0`, `82201558` filter | `havok.rs`, `world.rs` |

### Partially complete

| Area | What works | What is missing |
|---|---|---|
| **Lip tricks** | Rails loaded from the level; rail search; rail grab in the air with Y held; lip entry checks and snapping; lip state; balance meter; ollie out; falling off the meter; the `liptrick` → `InvertTrick` → `LipOut`/`OllieLipOut` scripts run for real | The **trick queue** (`SetQueueTricks`, `DoNextTrick`, button triggers) is not translated, so every lip is the default Invert (`DefaultLipTrick`); bails (`LipBail`); animations; score. |
| Rails | Build (`82197138`, `821939F8`) and search (`821968F8`) for normal levels | Grinds (`820F8120` grind set-up, `820F4DE8`, `820F8CF0`), single-node rails (`820F4108`), moving-object rails, park-editor paths, `CreatedFromVariable`/`createdfromtod` rails |
| Balance meter | Lip meter start/update/stop (`820CF748`, `82190C10`, `82190F58`, `820CEAE8`), cheese wear-off (`820D4A20`, `82190B10`), combo-end reset (`820CE840`, `821909A0`, from `ClearPanel_Landed/Bailed`, balance part only); **on-screen meter** (`82178D78` via `821795A8`/`821795B0`, layout `82175D08`, safe sides `820E5988`, the scripts `show/hide_balance_meter`, `update_balance_meter_colors`), shown for lips | Grind-only parts (same/new rail timing, robot rail), manual use (manuals not translated), pausing (`820CE618`), cheats. Grinds and manuals will show the meter as soon as their meters update: `show_on_screen` picks the manual (vertical) layout from Up/Down buttons, and `update_balance_sides` already has the Manual/Grind branches |
| Air update `820F2310` | See above | Wallride/wallplant (`820EDAA8`, `820E8618`, `820E80D8`), pitch bail (`820F31E4`), high ollie `820D79F8`, lip check `820EA788`, bikes, moving platforms, nose/tail contact feelers `820E5250` |
| Air spin `820E9620` | Spin and lean | Vert auto-turn, SmoothSpin, Nail the Trick |
| Ground update `820F6978` | See above | **Ground side collision `820EB9A0`**, manuals branch, skitching, high ollie, several animation/bookkeeping calls |
| Script commands | 64 translated in `skater.rs` (list: `COMMANDS` const) plus VM built-ins in `vm.rs` (`is_vm_command`) | The rest remain: animation, sound, trick system, scoring, UI, goals, walking, ragdoll, bikes. Untranslated commands return "not handled" and are listed in the game's HUD. |

### Not started

Grinds, manuals, flip/grab tricks and the trick system (queue, triggers,
double taps, trick names, scoring), bails and ragdoll, wallrides and
wallplants, walking, skitching, special meter, Nail the
Trick, skater model, animation system (anim tree, clips), pushing (depends on
animations), retail camera for grinds / lips / bails / wallrides (ground and
air done, NOTES 39), textured level rendering, audio, menus, stats menu.

## 6. Important files and what they do

| File | Contents |
|---|---|
| `crates/p8-skater/src/core_physics.rs` | `CorePhysics` (all physics state, with retail offsets), `Event`, `State` (Ground/Air/Lip), `Vert`, ground update, speed limits, friction, steering, backwards flip, tests. |
| `crates/p8-skater/src/air.rs` | `set_state` (part of `820D71B0`), Jump, air update, air leveling, wall collision, landing, and `step()` (the per-frame dispatch). |
| `crates/p8-skater/src/ground.rs` | Ground move loop, forward collision, wall response, wall push, ground snap, `orient_to_ground` (`820D7648`). |
| `crates/p8-skater/src/vert.rs` | Vert takeoff/air/tracking/break-vert, normal easing, uprighting, `rotate_about_row0/at`. |
| `crates/p8-skater/src/transfer.rs` | Spine transfer, acid/bank drop, orientation blend, post-transfer speed; the `Transfer` struct (SkaterState `+136`, `+192`, `+200`, `+272`, physics `+1380`, `+2130..+2134`, `+2172`, `+2224`, `+2304..+2464`, `+2546`, `+2616`); `ScriptAction` (scripts the physics asks to run). Tests with a synthetic spine. |
| `crates/p8-skater/src/rails.rs` | `RailManager::build` and `search`. |
| `crates/p8-skater/src/lip.rs` | Rail check (`820FAAA8`), may-take-rail (`820DCBE8`), grab (`820F8120` up to the lip), lip entry (`820F44C0`), lip update (`820F49D8`), `SkateInAble` (`820E55E0`), `random()`. |
| `crates/p8-skater/src/balance.rs` | Balance component and meters; `show_on_screen` = end of `82190F58`. |
| `crates/p8-skater/src/meter_display.rs` | The meter on screen: `MeterLayout` (`82175D08`, `balance_meter_info`), `MeterDisplay::set` (`82178D78`), `sprites()` (what `create_panel_stuff` / `do_show_balance_meter` / `update_balance_meter_colors` leave on the four sprites). |
| `crates/p8-formats/src/texture.rs` | `.img` decoder (Xbox 360 tiled DXT5, packed-mip offset) and `load_img(pak, name)`. |
| `crates/p8-skater/src/skater.rs` | `Skater` (physics + running script), the script host `Ctx` with all translated skater commands, event name mapping, `COMMANDS` list. `P8_TRACE=1` prints every command the scripts run. |
| `crates/p8-skater/src/script.rs` | `Scripts`: globals, `physics_float`, `global_float`, `stat`, `stat_value_of`, terrain lookups. |
| `crates/p8-skater/src/controller.rs`, `input.rs`, `pad.rs` | Controller path and `InputState`. |
| `crates/p8-skater/src/world.rs` | `World` trait (`feeler`, `rails`), `FlatFloor`, `Level` (Havok collision + rails). |
| `crates/p8-script/src/vm.rs` | Script VM; tests with a tiny assembler. |
| `crates/p8-script/src/params.rs` | `Params` (retail `CStruct` semantics: AddComponent, lookups, `get_in` with struct includes, `resolve_alias`). |
| `crates/p8-formats/src/zone.rs` | Zone loading: collision, restarts, rail nodes, compressed-node template expansion. |
| `crates/p8-game/src/translated.rs` | Bevy play mode: level mesh from collision, placeholder skater and camera, HUD, input. |
| `crates/p8-skater/src/vibration.rs` | Controller rumble component (`Vibrate`, timers, pad levels). |
| `crates/p8-game/src/rumble.rs` | Sends the rumble levels to the connected controllers. |
| `crates/p8-game/src/balance_meter.rs` | Loads `balancemeter_bg`, `balancemeter`, `balancemeter_2`, `balancearrow_glow` from `ZONES/global.pak.xen`; places them as UI images each frame. |
| `crates/p8-skater/examples/*.rs` | Headless test rides on the real level: `level_ride`, `script_ride` (real scripts), `vert_ride` (a halfpipe; `up` holds Up), `lip_ride` (holds Y; `ollie` ollies out), `transfer_ride` (`spine` or `acid`, R2 held; physics only, the award scripts are printed). |
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
8. **Placeholder visuals** (level as grey shapes). The camera is the retail
   skater camera since NOTES 39 (`p8-skater/src/camera.rs`).

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
- OverrideLimits friction `+2096` and gravity `+2100` before any
  `OverrideLimits` call: UNKNOWN; no writer besides `820D5C68` was found, so
  they start at 0 (INFERRED zero-filled memory). They matter only while the
  post-transfer allowance (`820DAFF0`) runs, which turns the override on
  with those values. Only the skitch scripts call `OverrideLimits`.
- `820EA0D0`'s delta order (`last^-1 * m` in retail row order): INFERRED
  (the only order that lands on the target when nothing else turns the
  skater); the matrix slerp's near-parallel branch is glam's, not read.
- `p8-sim` values (`tuning.rs`) are the old prototype's: TEMPORARY, not used
  in translated mode.
- **Balance meter drawing** (`p8-game/src/balance_meter.rs`): the retail
  screen element system is not translated. The 640 x 480 HUD is scaled by
  the window height and centred: APPROXIMATE. Colours: RGBA / 128 (INFERRED
  from the scripts using [128 128 128] as untinted; the draw-time
  conversion was not found). Retail angles turn clockwise on screen
  (INFERRED: the arrow then leans the way the arc drops). The container has
  no dims, so its size is 0 (LIKELY). Global flag `NO_DISPLAY_BALANCE` is
  taken as clear (global flags not translated) and
  `FLAG_SKATER_LIPTRICK_CAM_REVERSED` as clear (set only by the retail
  camera `820D1238`).

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
- **Scripts the physics runs** (`822265F8`: the transfer award scripts,
  `SkaterAcidDropTriggered`) and its `ClearEventHandler Ollied` run right
  after the physics step instead of inside it (APPROXIMATE timing). A run
  script that does not finish is kept and updated each frame
  (`Skater::spawned`, INFERRED).
- **Transfers**: level objects are not loaded, so `+2546` (target on a
  moving object) is always false; `+1618` `auto_drop` and `+1619`
  `drop_backwards` are only set by untranslated code (ollie state
  `820F01E8`, wheelies, bikes). The rail grab's use of `+192`/`+2768`
  (`820F8454`) is not translated. The surface flag 0x100 ("bank") name
  and the feeler mode word 0x4018 are UNKNOWN.
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
  not exist. `README.md` was rewritten (no stages; screenshots from the
  user's video, approved by the user though they show the game's model). `zone.rs` calls struct includes "LIKELY"; the
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
`INSPECT.bat` runs `p8-inspect`. `INSTALL.bat` checks for Rust and the C++
build tools; `TEST.bat` (was `CAMERA_TEST.bat`) rebuilds, prints
`TEST_CHECKLIST.txt` and plays.

macOS (user, same steps): the `.command` twin of every launcher above
(`INSTALL.command` checks the Xcode Command Line Tools and rustup). Game
data on the Mac: `~/Project8Data/DATA`, copied from the PC or unpacked with
the private repo's `backup/restore-mac.command`. No rumble on macOS (gilrs
0.6.8 `is_ff_supported` is false there). Any launcher change goes into both
files of the pair in one commit; checklist text only in
`TEST_CHECKLIST.txt`.

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

`p8-game` in the cloud: first
`apt-get install -y libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev pkg-config`,
then `cargo check -p p8-game` works (NOTES 39).

Note: `cargo test --workspace` in one go may exceed a 10-minute tool timeout
because of the Bevy test build; run the crates separately.

## 16. Current build status

- `cargo clippy -p p8-formats -p p8-skater -p p8-game --all-targets`: clean.
  p8-game now builds in the cloud container after
  `apt-get install libwayland-dev libudev-dev libasound2-dev libxkbcommon-dev`;
  it also runs there under `Xvfb :99` with `mesa-vulkan-drivers` and
  `libxkbcommon-x11-0` (screenshots with ImageMagick `import -window root`).
- Tests: p8-skater 60 (2 new in `meter_display.rs`, cheese wear-off, combo-end reset, lip rumble, 3 in `vibration.rs`), p8-script 9,
  p8-formats 10 (2 new in `texture.rs`), p8-game 1 + 2.
- Balance meter: `lip_ride` now prints the meter's on-screen state. On
  z_houses the meter appears when `DoBalanceTrick` runs, the arrow follows
  `arrow_positions` to (80, 5) turned 43.5 degrees just before
  `OffMeterBottom`, and it hides when the lip-out script stops the balance.
  Left = danger, right = safe on that lip (matches `InvertTrick`: Top ->
  LipBail unless `SkateInAble Lip`). Screenshots of the real game (with the
  display forced on by a temporary local change, not committed) showed the
  arc, the lit half and the arrow in place. The `.img` decoder's output is
  byte-identical to the research tool's decode.
- User report after playing: "the meter doesn't pop up immediately, and
  when it does it's already to one side". (1) The delay is retail:
  `InvertTrick` waits `<AnimData>.init_skip_time` (Invert 0.7667 s) before
  `DoBalanceTrick`, while the invert's opening animation plays (animations
  are not in yet, so nothing shows during it). (2) The off-centre start was
  a missing translation: the cheese (`+68`, added to the lean at each start
  by `82190C10`) never wore off. Fixed with `820D4A20` / `82190B10`
  (`Balance::wear_off_cheese`, called from `step()` after the transfer
  allowance): it drops by Cheese / CheeseFrames per 60th of a second (lip:
  3 s to zero).
- User follow-up: the cheese must reset when the combo ends. Confirmed:
  `ClearPanel_Landed` (`82124BA8`) and `ClearPanel_Bailed` (`82124EC0`)
  end with `820CE840`, which resets every meter (`821909A0`: cheese, lean,
  lean speed, times) and the running type. Landing runs them through
  `landskatertricks` (`Land2`, `LipOut`/`OllieLipOut` on the ground,
  `BailSkaterTricks` for bails). Translated as `Balance::reset_all`; the two
  commands are handled in `skater.rs` **for the balance part only** (their
  score, gap and event parts wait for the score system, so they no longer
  show in the untranslated-commands count). So now: the cheese wears off
  over CheeseFrames during a combo, and is cleared when the combo ends.
- `transfer_ride` on z_houses: `spine` finds the spine at x ~ -65, flies
  over it and lands down the far side at 12.7 m/s with `LandedFromSpine`;
  `acid` rolls off a deck at 6 m/s, pops to 5 m/s, drops into the ramp
  below (DropHeight 1.65) and lands at 12.7 m/s. `vert_ride` and
  `lip_ride` output is identical to before.
- `check_scripts`: 7626 scripts walk cleanly.
- Real-level rides: `vert_ride` (up and down a halfpipe, break-vert with Up),
  `lip_ride` (lip grab, ~1.7 s until the meter tips with no input, ollie out
  to ~2 m) behave as described.
- The user built and played up to the vert commit and reported quarter pipes
  feel right; the lip build has not been played by the user yet.

## 17. The exact next item and what I intended to do

The on-screen balance meter (asked for by the user) is done for lips; it
needs no extra work for grinds and manuals beyond translating their meter
updates (call `Balance::show_on_screen` after them, as `lip_update` does).
The next item is unchanged:

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

## 17-tricks. Trick system (done this session; the user's priority now)

The user asked for gameplay before stats: the trick system, manuals,
grinds, flip tricks, grab tricks, wallrides. Stats wait.

- DONE (p8-skater/src/trick.rs, NOTES 28/29): button events, every
  trigger type the data uses, the queue, DoNextTrick, extra / manual /
  grind pending tricks, the trick mapping from the profile (HawkTricks),
  the trick commands; flip/rotate-after flags, DoingTrick, matrix queries
  (queries.rs), GetArraySize / SetArrayElement in the VM, the manual
  balance meter in the ground update. Checked with
  `P8_FLAT=1 P8_TRICK=<left|up|grab:down|ground:r2|ground:manual> cargo run
  --release -p p8-skater --example anim_check -- <DATA/COMPRESSED>
  /home/user/p8work/extra_names.txt`: kickflip, tail grab, R2 stance switch
  (the user's video: it now keeps rolling forward and switches), manual
  (leans and bails with no input).
- Also fixed from the user's videos: handplant hold looped (wobble node
  translated), handplant bail never got up (anim_command /
  Skater_AnimComplete, timer_wait now blocks scripts), board rode
  backwards after a backwards landing (820B0E20 board part).
- Fixed next from the user's videos (NOTES 30): stance after a 180
  (Skater_Anim_Command refreshes the inputs before building a branch;
  boardrotateoverlay + Pose::board_rotate); manual trick transitions
  (Manual -> One Foot Manual / Truckstand / Anti Casper play their
  Exit/Entr clips: FormatText, AppendSuffixToChecksum, GlobalExists in
  the VM, Doing/StartBalanceTrick, Set/GetLastAnimData; SetExtraTricks
  ignore is text); a tapped grab on vert lasted until landing (grabout
  timer, skateridleswitch, object tags); bail falls looped each second
  (differencetoggle stand-in; now a pass-through). The game build broke
  once from an Rc in the anim tree: run `cargo check -p p8-game` after
  touching anim_tree.rs (the tree must stay Send + Sync).
  More anim_check modes: `ground:manual:<s|c|t...>` (buttons during a
  manual), `hold` (grab held to the ground: a landing bail), `spin:<deg>`,
  `P8_TREE_AT=<frame>` / `P8_AIR_AT=<air frame>` for the tree dump.
- Fixed next (NOTES 31): nollie popped from the tail (the VM lacked
  `FlipAndRotate` 820FDAF0, which Skater_PlayOllieAnim runs in nollie);
  boneless height (`Jump BonelessHeight` -> Physics_Boneless_*_Jump_Speed
  stats); `LastWasJumpBoneless` (+2544, set by BonelessHeight or NoComply),
  which Skater_HandleOllieModulation uses so tricks after a boneless /
  no comply fade the ollie foot layers (feet flailed before);
  GetScriptedStat reads a linked global struct (flip speed stat).
  No comply has the plain ollie height in the retail code (NoComply only
  sets +2544).
- Buttslap fixed (NOTES 31): `ClearException` is a script in events.qb,
  not a no-op; the VM's built-in no-op made the TrickOllie handler last the
  whole trick. Now the extra ollies end at 333 ms of air time / 15 frames,
  matching the user's retail clip (2 pops). Test:
  `P8_FLAT=1 P8_LEDGE=8 P8_TRICK=buttslap[:<frames>]` prints `jumps: N`.
- Stats: the skater now uses the profile's stat levels (821984C8; Hawk:
  air 11, spin 11, speed 10, ...) instead of 5 for all. This made the
  boneless FS 360 match retail and changes every stat-scaled value
  (speeds, spin, balance, switch factor) to Hawk's.
  New anim_check modes: `nollie[:<mode>]`, `boneless[:<dir>]`,
  `nocomply[:<dir>]`, `buttslap[:<frames>]`; `P8_LEDGE=<m>` makes the
  flat floor drop 3 m that far from the start.
- Ride direction glitch fixed (NOTES 32): after a ground backwards flip
  (`820DBAA8`, e.g. rolling back down a ramp) retail spawns
  `flip_skating_backwards` (= `Skater_PlayOnGroundAnim`), which rebuilds the
  ground animation; we never ran it, so the skaterflip node kept the pose
  turned round and the skater was drawn facing against its motion.
  `flip_if_backwards(s, landing)` now queues the script (landing /
  skating / manualing). Tools: examples `dir_fuzz` (random inputs, looks
  for backwards riding) and `bail_dir` (`P8_BACK=<t>`, `P8_NOGRAB=1`,
  `P8_SPIN=<deg>`). Known: `in_bail` (+128) is never set true.
- Bails / half pipe / manual / spacewalk (NOTES 33): `InBail` (820D8868)
  sets `in_bail` (state 9 = ragdoll bone update 820EAED0 not translated);
  the normal easing 820DA1A8 now moves the display matrix (stale upright
  matrix made vert landings bail); the vert auto-turn (820E9620) faces the
  skater down the wall in vert air; `DoNextTrick` / `DoNextManualTrick`
  pass their params to the trick script (`FromAir`, so a manual after a
  landing plays its landing) and `GetLastInAirVerticalVelocity` exists;
  anim events fire from `cycle` / `play` timers with `anim_events = on`
  (`AnimTree::fired`, `Skater::launch_anim_events` after `anim.update`, the
  game calls it in translated.rs): the spacewalk boost works. Not fired yet:
  `skatertimer` events (KickBoostEvent). Tools: `dir_fuzz`, `bail_dir`,
  `manual_snap`, `spacewalk_check`, `dump_global`, `find_ref` examples.
- Revert direction (NOTES 43): script `revert` (vert landings faster than
  6.35 m/s, `Land2` opens the `Reverts` window until it ends) picks FS/BS
  from the object flags `FLAG_SKATER_REVERTFS/BS` (lip inverts, some flip
  tricks) then `LastSpinWas frontside`. Both were untranslated (answer
  false), so it always did a FS revert: the direction depended only on the
  stance. Now: `LastSpinWas` (`820D5908`, core `+2217` =
  `CorePhysics::last_spin_positive`, written by the air spin `820E9DF8` and
  the ground turn `820ED548`) and `Obj_SetFlag/ClearFlag/FlagSet/FlagNotSet`
  (object `+44` bits, `821C8510`; command-to-case mapping LIKELY). Example
  `revert_check` `P8_PUSH=6 P8_TAP=3 P8_SPIN=180 [P8_R1=1]`. R2 when slower or
  later is the cess turn `ToggleSwitchRegular`: its side comes from
  `LeftPressed` / `RightPressed` (now translated: input +96 / +128 held,
  `82259928` / `82116A10`), but the game's `BSCess_*Data` use the same
  frontside clips as `Cess_*Data`, so it looks the same either way (retail).
- Choppy animation (NOTES 42): `skater_model::animate` sampled the whole
  tree (body and board) every drawn frame from the tick's state, with no
  blending, so at ~58 fps the pose jumped two ticks every ~35 frames
  (the user's clip: one frozen frame about every 35) while the root and
  camera were smooth. Now `translated::step` samples once a tick after the
  anim update and `animate` blends the last two poses by the overstep
  (not on the turn-round tick). Not confirmed in the game yet. Open: the
  skatertimer's `sync` read treats an unset parameter as on; the newer nodes
  treat it as absent (INFERRED both; check 820A2870).
- Branch audit (NOTES 44, overnight): the per-frame functions 820F2310,
  820F6978, 820E9620, 820ECEE8, 820F12C0, 820F0730, 820FC990, 820B4038,
  820B2790 (and inits 820B2D00, 820B3D08) now cite every branch (0 uncited):
  translated, or "not translated" with the reason. Fixed on the way: vert
  auto-turn only with no spin input, spine / acid landing keeps the aimed
  velocity, manual uphill threshold, ground frames clear flipping / lean, air
  spin no longer writes +1940, skatermodulate spin + grindlean, skatertimer
  turn. Not fixed (next sessions, need the user in the game): side collision
  `820F2238`/`820ED630`, head / ceiling check `820EA788`, lean display and
  force_flip_bail, level node TriggerScripts (`8228C880`). Camera update
  820D1238 still 97 uncited. The audit tool now counts a call as cited
  through its target only if the target is cited on a line without "not
  translated".
- Process (NOTES 41): `CLAUDE.md` (loaded by every session) holds the
  session setup, the user's rules, the "before calling a feature done"
  checks and the known bug shapes. Branch audit:
  `python3 /home/user/project-8-data/tools/branch_audit.py <addr> crates/<crate>/src`.
  First audits still to do: camera update `820D1238` (97 of 250 uncited,
  mostly the grind / lip / wallride / bail paths), modulate init
  `82381C18` (3), takeoffblend init `820B5950` (12), ollielandblend
  `820A8050` (18) / `820A7D88` (25). Every older function too: most cite only
  their start address, so run the audit on a function before changing it.
- `id` + `sync` sweep (NOTES 41): takeoffblend (`820B5950`/`820B5890`) and
  ollielandblend (`820A8050`/`820A7D88`) also carry `t` through the tag named
  by their id (a flip trick rebuilds the ollie branch mid-air with
  `sync = 1`, `Skater_PlayFlipTrickAnim`). Fixed with a unit test; no
  headless run covers that path yet. `speedtimerthreeway` (walking) has it
  too, not translated.
- Spacewalk hips (NOTES 40): the hips and feet swung about 90 degrees out
  and back as the spacewalk started. Cause: `modulate` init (`82381C18`)
  with `id` + `sync`: sync 0 sets the tag named by the id to 1, otherwise
  the strength starts from that tag; the update (`823819D8`, `82381AAC`)
  writes the strength to it while blending. We ignored both, so the new
  Manual_AnimBranch's `manualbalancemod` / `manuallandmod` /
  `manualslopemod` started at 1 where the exit (`Manual_out_1`) had faded
  them to 0. Fixed in `Build::modulate` / `Modulate::update`, unit test,
  `spacewalk_check` `P8_POSE=1`. Every manual transition entry is affected
  (pivot checked: same steps). Not confirmed in the game yet.
- Skater camera (NOTES 39): retail `820D1238` translated for ground, air and
  vert air in `p8-skater/src/camera.rs` (`SkaterCamera`), modes from
  `Skater_Camera_Array` (mode 2, Standard_Medium, `820CFB30`), follow frame,
  tilt + air tilt, slerp with the turn limiter, focus lerp (`820D0F48`),
  zoom/above (`820D02A8`), look-at and level roll. `p8-game` translated.rs
  uses it (placeholder chase camera removed), fov from `horiz_fov`
  (INFERRED horizontal). Not yet: grinds, wallrides, lip turn `820D0780`,
  bail/ragdoll camera, look-around, camera collision `820BBF58`. Example
  `camera_check` (`P8_VERT=1`). User test: `TEST.bat` / `TEST.command` (then named `CAMERA_TEST.bat`). Not confirmed
  in the game yet.
- Manual pivot snap-back (NOTES 38): R2 in a manual (`Trick_Gturn` /
  `Trick_Gturn2`) turned the skater with the clip and then snapped back
  because the stance never flipped. Cause: `StructureContains` (`822ACAD0`)
  did not follow a name to a global struct (retail: typed getter
  `82212218`, globals first, then the locals), so `flipafter` on the
  transition data was never seen. Fixed in `p8-script` `vm.rs` with a unit
  test. Example `pivot_check`. Not confirmed visually. Also affects
  `use_anim_length` / `boardrotate` on transition data.
- Revert animation (NOTES 37): `skatertimer` init (`820B3E04`) with `id` and
  `sync` starts at end * the tag (sync 0 clears it); we ignored it, so the
  ground animation after the revert's `flip` restarted at 0. Fixed in
  `Build::skater_timer`. Example `revert_check` (`P8_TIMERS=1`, `P8_CLIP=`).
- Spine transfer bail (NOTES 36): every spine transfer landing bailed because
  the display matrix (`+32`, read by `PitchGreaterThan` etc.) stayed on the
  take-off wall: retail's air update (`820F3018`) eases the normal (and `+32`)
  in vert air only when there is no spine transfer (`+136`); else `+32` is
  the object matrix. `follow_display_matrix` has the term. Example
  `spine_bail` (real scripts, prints the land checks) is the regression run.
- Spin timers (NOTES 35): `skatertimer` `spin` (44A8) and `vertspin`
  (4208) of `820B4038` are translated (`SkaterTimer::update`; angle from the
  trick spin / from `821ED9E8` between the at row and the velocity, time =
  angle / 360, vertspin limited to 0.015 a frame). Root cause of "stuck at
  t=0": `ClipLib::duration` returned 0 for a missing clip; retail `822448D0`
  returns 1.0. Fixed. Tool: `P8_TIMERS=1` with `P8_SPIN=1` in `anim_check`.
  `spin`'s `nollie` half turn is APPROXIMATE. Timer types turn, brake, grab
  still not translated.
- Still untranslated nodes the HUD lists: overlay, walkmonitor, walkspeed
  (pass-through stand-ins), differencetoggle's "on" difference (walking
  only). Ragdoll is not translated: after a bail the skater slides on the
  physics until the get-up.
- TOOL: `research/tools/harness` (private repo) compiles recompiled retail
  functions and runs them on test inputs. Use it to check a translation or
  to find what a long function does. README there.
- NEXT (user's list): grinds (rail grab set-up 820F8120, grind update
  820F4DE8, rail update 820F8CF0, the balance meter's grind parts; trick
  lists GrindTricks etc.), wallrides (WallRideTricks, core wall ride),
  reverts (Reverts list via ExtraSlot1/2 -> Trick_Revert: should work,
  untested), landing bails (the checks are translated; test a trick landed
  late), special meter ([+28]+116, special lists), score display
  (Display, SetTrickName/Score are stored only), the board model's
  rotate (820FD838 [+40]+520), skitch.
- APPROXIMATE / INFERRED here: event times use game time (retail real
  time, same unpaused); DoNextTrick goes to the calling script (retail
  the skater's main script); scripttorunfirst run at once; stick-direction
  buttons (ids 19-34), L3/R3 never held (no records); PLAYER_SKATER =
  hawk.

## 17-model. Skater model and animations (in progress; the user chose this before the trick queue)

Plan agreed with the user: (1) skeleton, (2) model + textures in the game,
(3) animation clips (decode `.ska`), (4) the animation system (script
commands, anim tree, waits), then tricks on top, then the ragdoll bail.

- Done (1)+(2): `p8-formats` `skeleton.rs` (`.ske`), `scene.rs`
  (`.skin.xen`), `texture.rs` dictionaries + DXT1 (`.tex.xen`), example
  `model_check`; `p8-game/src/skater_model.rs` shows the `Pro_Hawk` profile
  (skeleton `Pros_Hawk_skel`, mesh from the `ped_body` table) as a skinned
  mesh in bind pose instead of the capsule (the placeholder board stays).
  Formats in `docs/formats.md`.
- (3) in progress: `.ska` clips are in `PAK/perm_anims.pak.xen` (5255),
  compressed; header at +0x20: 0x28, flags, duration (float), bone count
  (u16), key counts, offsets. The quantisation tables come from
  `DATA/ANIMS/standardkeyQ.bin.xen` / `standardkeyT.bin.xen` (script
  `load_permanent_assets`: `InitAnimCompressTable ... q48 / t48` ->
  `82296D70` -> tables at `82777000` / `82777800`). Decoders: `822EC118`
  (893 instr.) and `822EDB00` (972), helpers `822EA2C8`, `822ED7C0`;
  samplers `822ECF10` (from `82375AE0`) and `822EEA30` (from `822CA1E8`,
  `823757A0`). Translate these; do not guess the bit packing.
- (3) done: `p8-formats/src/anim.rs` reads `.ska` clips: the compressed
  keys (translation of `822EF058` + `822EEF98`, fullres cache mode 4),
  single-pose `_xx` clips (`822EA3D0`), the bone mask, the key search
  (`822E90B8` rotations, `822E9330` positions), the sampler (`822EB590`,
  including its frame clamp that carries over to later bones) and the
  quaternion blend (`822EAEB0`, nlerp with hemisphere flip). Verified: the
  Rust keys equal the retail decompressor's output (run in unicorn) on all
  28 reference clips (example `anim_dump`; references in the private repo
  scratch notes, NOTES section 24).
- Pose rules (read from code, not guessed): stored rotations are
  conjugated before matrices (`82327678`), like the skeleton. `_xx` clips
  replace; `_xDx` clips are differences applied by the ApplyDifference node
  (`8237B1D8`/`82377058`/`823767C8`): rotation = nlerp(base, base ⊗ diff,
  w), position = base + w·diff (`anim::apply_difference`). Add node
  (`82376220`): q = qB⊗qA after nlerp from identity by weight; t = tA·wA +
  tB·wB (not in Rust yet).
- (4) in progress: the animation tree runs from the scripts
  (`p8-skater/src/anim_tree.rs`, commands `Skater_Anim_Command`,
  `Skater_AnimNodeExists` in `skater.rs`; the game samples it in
  `skater_model::animate`). Verified with `cargo run -p p8-skater --example
  anim_check -- <DATA/COMPRESSED> <names>`: the scripts add
  `OnGround_AnimBranch` at spawn, `Stopped_AnimBranch` when still,
  `Ollie_AnimBranch` on the ollie (set `P8_POSE_DUMP=<dir>` to dump poses).
  - Translated (addresses in the module doc): pose ops (source 823757A0
    fills the rest pose first; flush 82376BF8; add 82376220 = A ⊗ B;
    apply difference 823767C8; blend 82376648), nodes source/skatersource,
    cycle, play, add, applydifference, modulate + blend functions (curve
    stores 1 - v), degenerateblend (default blend 0.3 s), ik
    (`p8-formats/src/ik.rs`: Havok two-joint solver 824E9118, hinge
    (0,1,0), gains 1; target = the animation's Bone_IK_Foot_Slave; ankle
    takes its rotation), skaterflip (mirror + turn-round, see STANCE),
    boardrotateoverlay as pass-through, skaterposecapture, skatermodulate
    `offwhenfinished`.
  - User report fixed: feet drifted off the board because there was no IK.
    User asked whether the idle was right: it was not; standing still is
    `Stoppedstate` -> `Skater_PlayStoppedAnim` -> `Stopped_AnimBranch`
    (sk8_gnd_Stop_base_xx + sk8_gnd_Stop_idle_xdx + from_stnd), now played.
  - DONE since: every node type in the ground, stopped and ollie trees
    (rolling: skatertimer, speedblend, crouchblend, ubercrouchblend,
    kicktimer/kickcatch, braketimer/brakecatch, skatertimedswitch; air:
    blank, partialswitch, apextimer, takeoffblend, ollielandblend,
    spinleftrighttimer, spinleftrightadd), skatermodulate's modes, the
    animinfo inputs (`CorePhysics::anim_inputs_in`: brake input 820D7570,
    time to land 820E2CD8, time to apex 820D7878), and the real board
    (`board_default` on the `board` skeleton, sampled from the same tree with
    `<clip>_b` clips; `qb_key_extend` = 821E5718). Addresses: NOTES 25-26.
  - IK hinge: retail's constant is (0,1,0) in the ragdoll's Havok skeleton
    (824E47E8). CONFIRMED to be our +Z: every body of
    `default_human_ragdoll.rag` (global.pak, Havok 4.0 packfile) sits at our
    bind position turned -90 degrees about the bone's X. Documented in ik.rs.
  - Board sampling steps the same sample-time state as the skater's
    (82245DC0/82245E58); pose captures are per object (820B3798/820B0380).
  - STANCE DONE (the 180 snap): SkaterState +40 = `CorePhysics::flipped`,
    +48 = `rotated` (both toggled by the backwards flip 820DBAA8 and
    FlipAndRotate 820D9008/820FD8E8); a restart sets flipped = the
    profile's goofy flag (820DFB88; `master_skater_list` entry `hawk` is
    goofy, `PLAYER_SKATER` APPROXIMATE until skater choice is read);
    switch = 820B8220. skaterflip mirrors its child if flipped when built
    (823835D0) and turns its pose round if +48 changed since (820B0E20 ->
    820B0560 on bones 1, 89, 91). Checked with `P8_SPIN=1` on anim_check:
    feet direction changes 0.4 degrees on the landing frame. NOT done: the
    board's part of 820B0E20 (HUD lists `skaterflip` when it would run);
    the display matrix smoothing 820DA1A8 (we still draw from the physics
    matrix; the game skips its own between-ticks smoothing on a turn).
  - User report (video, stand-in build): feet through the board, wrong
    rolling poses. Causes: the stand-ins (base_transition looped), the Y
    hinge, and the invented board box. All three replaced.
  - Check tool: `P8_FLAT=1 P8_POSE_DUMP=<dir> P8_DUMP_EVERY=10 cargo run -p
    p8-skater --example anim_check -- <DATA/COMPRESSED> <names>` dumps poses,
    live tree values and the board. Render with the private repo's pose
    scripts (research/tools).
  - NEXT: anim events (`8237C178`/`8237C380`, the `skateranimeventtable`;
    `KickBoostEvent` gives the push its speed), grind/manual/lip branches
    (new node types will show on the HUD), the static tree's wheel/face
    layers, the board part of 820B0E20, display smoothing 820DA1A8.
  - FOUND, not done: the player's stats. `master_skater_list` `hawk` has
    his own stats (speed 10, spin 11, ollie 7, air 11, ...); the physics
    uses `StatLevels::with_default` (5 or the script default). Find how
    retail loads profile stats into the stat component before changing.
  - FOUND: `default_appearance` = `appearance_Hawk` is not in the dumped
    globals (search other paks); it may name the pro board.
- User report (model shown): after landing a 180 the model snapped to face
  forward. FIXED by the stance work above.
- Next after (3): the board model (`board_default`, own skeleton `board`
  in global.pak) and the animation system.

## 17a. Audit of the balance meter and lip stalls (done with the 17b method)

Every function of the balance component (vtable `820027F4`, commands in
`component_commands.tsv`) and of the meter (`82190930..82191840`), and every
command the lip scripts run on z_houses, was checked:

- Translated: DoBalanceTrick, StopBalanceTrick, meter start/update/stop,
  display, safe sides, cheese wear-off, combo-end reset (`820CE840`) and,
  added by the audit, `820CE7F8` (ClearPanel_Landed keeps the longest
  balance time before the reset). `balanceparams` handling matches retail
  (kept once given; cleared only by a skater reset `8219AD18` -> `820CF168`,
  which a restart covers by making a new skater).
- Same as retail, nothing to do: constructor `820CE520` (all zero),
  `820CE590` (component links).
- **Not done, found by the audit:**
  - Controller rumble: **done** (see "Controller rumble" below).
  - `FlipAfter` / `Rotate` in `OllieLipOut`: `FlipAfter` (`820FD738`) sets
    flipandrotate `+25`; the flip itself (`820FD8E8`) runs on the next
    `PlayAnim` event (`820FDCA8`) or `HandleFlipOrBoardRotateAfter`. Needs
    animation events, so it waits for animations. It decides which way the
    skater faces after an ollie out of a lip.
  - Trick queue commands (ClearTrickQueue, SetQueueTricks, DoNextTrick,
    KillExtraTricks, SetTrickName/Score, UseGrindEvents): the planned next
    task.
  - `Obj_FlagSet` (object flags): untranslated, so the flags read as clear
    (REVERTFS/BS, LIPTRICK_CAM_REVERSED).
  - Grinds/stalls: `AdjustBalance` (`820CEE88`, via
    `apply_acid_drop_cheese`), `SetWobbleDetails` (`820CF668`). Manuals:
    `StartBalanceTrick`, `SetBalanceTrickType`, `DoingBalanceTrick`,
    `AdjustBalance`. Pausing the meter `switchoff/onbalancemeter`
    (`820CE618`/`820CE6B0`, not used by scripts).
  - Already listed before: moving platforms in the lip update, the
    perfect-balance cheat, online play.

### Controller rumble (translated after the audit)

- `p8-skater/src/vibration.rs`: the "vibration" component. Command
  `Vibrate` (`8228D528`: Actuator 0 left/heavy, 1 right/light; Percent;
  optional duration; `OFF`), per-frame timers (`8228D260`), pad levels
  (`823A67E0`: 255 * percent / 100, sent as level << 8; max 255 from
  `823A6280`). Starts on (`default_system_startup: vibrationon`; the
  profile option is not read).
- `Balance::rumble_percent` = balance component update `820CF438`: while a
  meter leans, both motors at |lean|/4096*100*f + `min_balance_vibration`
  (10), f = speed capped at 1, 1 on lips. Run from `Skater::step` after the
  scripts, then the vibration timers (order among components LIKELY).
- The scripts stop it: `InAirExceptions` / `OnGroundExceptions` /
  `GeneralBail` / `ManualLand` run `VibrateOff` (`vibrate off`). Ollies,
  flip tricks, wallplants and landings call `Vibrate` with durations and now
  rumble too.
- `p8-game/src/rumble.rs`: sends level changes to every connected pad
  (Bevy `GamepadRumbleRequest`, Stop then Add held for an hour: the hold
  stands in for "until changed").
- On z_houses (`lip_ride`, which prints `rumble`): 25/255 when the lip
  balance starts, 255 just before falling off, 0 as the lip ends.
- User report: falling off the red side keeps rumbling until the next
  ollie. Cause (reproduced with `lip_ride ... left`): OffMeterTop ->
  `LipBail` -> `GotoRandomScript [InvertBail]` is not translated, so the
  script stops in LipBail; retail goes InvertBail -> Bail_NoInit ->
  `GeneralBail`, which runs `VibrateOff` (line 134 of its 250) and then the
  ragdoll bail. So the skater does not bail at all yet, and nothing stops
  the rumble. Fix = the bail system (roadmap item 5), not a stand-in.
- Then (user chose "bails now"): `GotoRandomScript` (`822A9078`) is now a
  VM command (first unnamed array of checksums, random pick via
  `821E8508`, goto with no params). The bail scripts now run: LipBail ->
  InvertBail -> Bail_NoInit -> GeneralBail (VibrateOff: rumble stops) ->
  Baildone -> the skater lands and ends in `Stoppedstate` (standing still,
  as retail ends a bail), and can get going again with stick/crouch. But the
  whole bail passes in one frame: its waits (`WaitForRagdoll`,
  `Bail_WaitAnim*`) and the ragdoll are not translated. Untranslated in
  GeneralBail: PausePhysics, RagdollBailActivate, Ragdoll_* (SetState,
  Anim_Set_State, FixMatrix, ...), WaitForRagdoll, Obj_SpawnScriptNow
  (BailBoardControl), Obj_Get/SetPosition, Obj_GetBonePosition,
  BailMoveAwayFromGeo, SetForcedBail, SetStandingBail, IsStandingBail,
  InBail, PlaySkaterStream, SetTags/GetSingleTag, ... In Baildone:
  UnPausePhysics, BailLerpToGround, BailOrientToBones, RagdollBailDeactivate,
  Ragdoll_BlendToInactive, BashOn/Off, SetSloMo. The crash itself (how long
  it lasts, where the skater ends up) comes from the ragdoll, which needs
  the skater's skeleton (models/animations, not started).
- Not done: pause blocking (`82778B54`, object `+308` bit 0; no pause yet),
  `VibrationOn/Off/IsOn` (options menu, player index), `VibrateController`
  and `Vibrate_Controller_Safe` (menus, special level objects), physics
  `+1548` in `820CF438` (UNKNOWN, taken as clear).

## 17b. How to check a feature is complete (the user asked for this)

The user should not have to find missing pieces by playing. Twice in one
session they did (the balance meter's cheese never wore off, and was never
reset at the end of a combo), because only the functions on the direct call
path were translated. For every feature, before calling it done:

1. List every field it reads or writes (with retail offsets).
2. Find **every** retail function that writes those fields (grep the
   generated code for the offset on that object, e.g. `stfs f\d+,68\(r`
   in `8219xxxx`/`820Cxxxx`, plus `callers.py`), and every script command
   that leads there (`component_commands.tsv`, `script_commands.tsv`,
   `qbscript.py all '.*' x` to see which scripts call it and when).
3. Translate each one, or label it in code and here as not done, and why.
4. Tell the user what is still missing, before they find it.

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
