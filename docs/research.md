# Tony Hawk's Project 8: research notes

Source: `theokyr/Project8Recomp` @ default branch (shallow clone, 90 files).
Confidence labels: **CONFIRMED** (stated in source or config), **LIKELY**
(strong inference), **UNKNOWN**.

## What the reference repository contains

Project8Recomp is a ReXGlue static recompilation of the Xbox 360 executable.
Its committed content is limited to:

- the launcher and host glue (`src/`),
- the codegen manifest (`config/thps_p8_manifest.toml`), and
- SDK patches.

**The recompiled game code is not committed.** It is generated locally from
each user's own `default.xex` (AGENTS.md: "Generated sources are never
committed"). QB scripts, animations and levels are also absent. So **no
gameplay constants, physics code or state machines are present in the
repository.** Everything below about skater *behaviour* is therefore UNKNOWN
unless it is marked otherwise.

## Engine facts

| Finding | Confidence | Evidence |
| --- | --- | --- |
| Neversoft "nxcommon" engine, with gameplay driven by QB scripts through a C++ script-command table | CONFIRMED | `guest_probe.h` header comment |
| Script-command table: 1,521 `{name, handler}` records at `0x826D9C68`, of which 144 are compiled-out stubs | CONFIRMED | `guest_probe.h` |
| QB name checksums are CRC-32 (poly `0xEDB88320`, init `0xFFFFFFFF`, **no final XOR**) over the lower-cased name with `/` replaced by `\` | CONFIRMED | `guest_probe.h::Checksum`; the recomp team verified 118,762 pairs |
| Script commands take `(CStruct* params, CScript* script)`. Parameters are keyed by checksum | CONFIRMED | `dev_cheats.h` |
| Local skater: `GetLocalSkater` at `0x822DF688` on the manager global `0x8276F3DC`. World position lives at skater `+0x70` (x, y, z, w floats) | CONFIRMED | `dev_cheats.h` |
| Havok is present (`DumpHavokMemStats` command) | CONFIRMED (presence) / UNKNOWN (whether the skater uses it) | `dev_cheats.h` allowlist |
| Asset containers are `.pak.xen` (for example `dbg.pak.xen` holds debug name strings) | CONFIRMED | `dev_cheats.h` comments |
| Skater bookkeeping commands: `InitializeSkaters`, `ReinsertSkaters`, `AllSkatersAreIdle`, `ResetComboRecords`, `WarpSkater` (the `nodename` parameter is a restart node) | CONFIRMED (names) | `dev_cheats.h` |
| Default keyboard mode needs `--mnk_mode=true`, and START maps to Escape | CONFIRMED | manifest header |
| Pad buttons exposed by the scripted-input layer: a b x y start back lb rb l3 r3 d-pad guide, plus lx/ly/rx/ry axes and lt/rt triggers | CONFIRMED (hardware layout only, not the game's mapping) | `rexglue_script_input.h` |
| The game logic is bound by a single guest thread | CONFIRMED | manifest comments |

## Skater behaviour

| Subsystem | Status |
| --- | --- |
| Input interpretation, ground movement, acceleration, turning, gravity, ollie, air control, manuals, grinds, wall-rides, trick system (including Nail-the-Trick), landing, bails, balance meters, animation states, camera | **UNKNOWN from this repository.** These need the locally generated code and the QB scripts from a user's own disc. |

The `p8-sim` skater therefore starts from **TEMPORARY TUNING
VALUES** chosen to reproduce the *genre* feel of arcade THPS-style skating
(fast responsive acceleration, gravity-scaled ollie, strong air spin). Every
tuning value will carry a provenance label. None will be claimed as original
values.

### How to obtain real values later (all on the user's own copy)

1. Build Project8Recomp locally. Use its console commands `script_find` and
   `cheat_position`, and use `peek` via `guest_probe.h`, to read live skater
   fields.
2. Resolve QB globals by checksum with the documented CRC (see the
   `ResolveGlobalSymbol` pattern in `dev_cheats.h`), then dump physics-named
   globals from the `.pak.xen` script data.
3. Record input and position traces with `rexglue_input_record.h` JSONL
   recordings and `cheat_position`. Fit speed, acceleration, gravity and jump
   height from those traces. Label the results MEASURED/ESTIMATED.

## Map-format notes for future map support

| Finding | Confidence |
| --- | --- |
| Levels are loaded via QB (`ChangeLevel` with a `level` checksum, for example `load_z_funpark` and `load_z_houses`) | CONFIRMED |
| Zone names: `z_funpark`, `z_houses` | CONFIRMED (two names) |
| Level data ships in `.pak.xen` containers. Scene, collision and node-array formats inside them are undocumented here | CONFIRMED (container) / UNKNOWN (contents) |
| Restart and spawn points are named nodes (`WarpSkater nodename=…`), which implies a NodeArray-style placement table | LIKELY |

Planned pipeline: the player's own installation, then
`p8-formats` readers, then a local converted cache, then the game. The
simulation only consumes triangles, rail segments and a spawn point, so
converted levels need no simulation changes.

## Original physics values (read by `p8-setup`)

CONFIRMED: `qb.pak.xen` holds the skater globals in a script that defines
`skater_physics` (a struct) plus top-level `physics_*_stat` items.
Stat-scaled values are structs with an unnamed `(min, max)` pair and a
`STATS_*` index; `skater_default_stats` is 5. The stat interpolation is
LIKELY linear over levels 0-10. `crates/p8-game/src/original.rs` lists
every value that is copied and how the simulation uses it.
