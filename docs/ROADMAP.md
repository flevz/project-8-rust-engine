# Roadmap

Each stage leaves a playable build.

1. **Prototype (done).** `p8-sim` skater, original test park, placeholder
   visuals, controller and keyboard input.
2. **File formats.** Confirm the `.pak.xen` archive layout with `p8-inspect`
   on a real installation. Then identify level scene, collision, texture,
   model, animation and QB script formats (see `docs/formats.md`).
3. **Level converter.** Convert a level (starting with Funpark) from the
   player's installation into a local cache: collision, visual geometry,
   textures, rails and restart nodes. Load it in the game.
4. **Skater model and animation.** Load the skater model, skeleton and
   animations from the installation, then drive animation from the
   simulation's pose states.
5. **Faithful gameplay.** Replace temporary tuning with values measured from
   the original. Sources: QB script globals, the Project8Recomp-generated code
   from the player's own executable, and recorded input/position traces.
   Every value is labelled CONFIRMED, MEASURED or TEMPORARY.
6. **Game systems.** Trick naming and scoring, special meter, Nail the Trick,
   career goals, menus, audio.

Rules: nothing from the game is committed. Converters run locally on the
player's own copy and write only to ignored folders.
