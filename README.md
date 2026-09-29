# Project 8 Rust Engine

An unofficial, from-scratch **Rust + Bevy** engine for **Tony Hawk's Project 8**
gameplay, in the spirit of the Skate 3 Rust Engine. It is not affiliated with
Activision or Neversoft.

**No game content is included, and none ever will be.** The engine reads the
scripts, levels, models and animations from *your own* installed copy of the
game at runtime.

The skater is a line-by-line translation of the original game code (from the
player's own disc, decompiled), not an imitation: each translated function
names the original address it comes from.

![Kickflip on the Houses level](docs/images/kickflip.png)

| | |
| --- | --- |
| ![In the air on the Houses level](docs/images/air.png) | ![Riding the Houses bowl](docs/images/riding.png) |

## What works

- **Original scripts:** the game's own QB scripts run in a translated script
  VM, so tricks, trick names, triggers and timings come from your copy.
- **Skating physics:** pushing, steering, braking, ollies (with the original
  jump speeds and tense time), nollie, boneless, no comply, air spin and
  lean, landings, walls, vert and quarter pipes, spine transfers, acid drops
  and bank drops.
- **Tricks:** the trick queue and button triggers, flip and grab tricks,
  manuals and manual tricks, lip tricks with the balance meter, stance
  switches and reverts.
- **Skater and animation:** the original skater model, board and animation
  tree (5,000+ clips), including foot IK and stance mirroring.
- **Levels:** the level's original collision and rails (shown as plain
  shapes; the textured level is not drawn yet).

Not there yet: grinds, wallrides, ragdoll bails, the score display, special
meter, walking, sound, menus and the textured level. Commands the scripts use
that are not translated yet are listed on screen while playing.

## Play (Windows)

Requires Rust (https://rustup.rs) and the Visual Studio C++ build tools.

1. Double-click `BUILD.bat`. The first build takes a while.
2. Double-click `SETUP.bat` and drag in your installed Project 8 folder. It
   remembers where your game files are (`scripts-location.txt`, kept on your
   machine only).
3. Double-click `PLAY.bat`.

Controls (the original Project 8 layout):

| Action | Controller | Keyboard |
| --- | --- | --- |
| Push, steer, lean, spin in the air | Left stick | W A S D |
| Crouch (hold) / ollie (release) | A | Space |
| Flip / grab / grind and lip tricks | X / B / Y | Y only: F |
| Spin | LB / RB | Q / E |
| Nollie (hold A) / switch stance | LT / RT | Left Shift (LT) |
| Manual | Stick up then down | W then S |
| Restart | Back | R |

## Help map the game files

1. Install Project 8 with Project8Recomp from your own disc. Its launcher
   copies the game data into its folder.
2. Double-click `INSPECT.bat` and drag that folder into the window.
3. Share `p8-inspect-report.txt`. It contains names, counts and sizes only.

## Layout

| Crate | Role |
| --- | --- |
| `crates/p8-skater` | The translated skater: physics, tricks, animation tree, controller path. |
| `crates/p8-script` | The translated script VM that runs the game's own scripts. |
| `crates/p8-formats` | Readers for the game's files, plus `p8-inspect`. |
| `crates/p8-game` | The Bevy application. |
| `crates/p8-sim` | The first hand-tuned prototype, used only when no game files are set up. |
