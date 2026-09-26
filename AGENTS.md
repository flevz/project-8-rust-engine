# Agent guide

- **Never commit game content.** That includes extracted files, converted
  output, disc images, executables, generated recompiler code and
  screenshots of game assets. The only files from the game you may see are
  structure reports such as `p8-inspect-report.txt`.
- Label findings CONFIRMED, LIKELY or UNKNOWN (`docs/formats.md`,
  `docs/research.md`). Tuning values are CONFIRMED ORIGINAL, MEASURED/ESTIMATED
  or TEMPORARY.
- Keep `p8-sim` free of Bevy and of file formats. Keep `p8-formats` free of
  Bevy. `p8-game` wires them together.
- Validate with `cargo test --workspace` and `cargo clippy --workspace`.
- The user is not a programmer: give plain-English steps and double-clickable
  `.bat` launchers for Windows.
