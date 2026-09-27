# Physics-to-script event ordering and waits

This follows up the event-dispatch prerequisite identified in the independent
audit of engine revision `796f420`. It does not implement the trick queue.

## Behavior

`Skater::step` now delivers physics events at their emission sites. A handler
can change the skater's velocity, state, or flags before the next physics
operation reads them. The returned event vector is an observation log, not a
queue to replay after physics.

Examples covered by synthetic regression tests:

- `Stopped` changes velocity before the ground move.
- `GroundGone` can disable the rail search later in the same step.
- `Ollied` can execute `Jump` before that rail search, with its broadcast
  recorded once, immediately after the triggering event.
- Lip entry executes its goto/update after placement and rail selection,
  before the terrain copy; an immediate script exit is not overwritten.

Ground, air, wall-response and lip-meter event sites use the same mechanism.
Broadcasts (`SkaterJump`, `SkaterOffEdge`) remain log entries; delivery to
other objects is not implemented. The standalone `CorePhysics` APIs retain
their collecting behavior and their existing script-free stand-ins.

## Wait behavior and audit correction

The subsequent private structure-only analysis at revision
`82ec1a7a8bcac36602461b61e224032b6778383e`
(`research/WAIT_FRAME_ANALYSIS.md`) resolves the wait question:

- **CONFIRMED FROM ORIGINAL DATA (address-annotated research):**
  `GameFrame` waits count interpreter updates, not simulation frames.
  `8221A270` sets type 1; `8221A568` decrements it on the issuing call and
  each subsequent update. Separate updates in the same simulation frame
  can consume the wait. The audit's suggestion that this alone was a
  fidelity defect was incorrect. No frame-counter guard has been added.
- **CONFIRMED FROM ORIGINAL DATA (address-annotated research):**
  `8220F914..8220F924` clears script +196 bit 0x80 on update entry;
  `8220F934..8220F948` skips a wait tick when the bit is set;
  `8220FC64/8220FC70` sets it on a waiting exit. This prevents an enclosing
  update from ticking a wait again after a nested update yielded. The VM
  now preserves that flag across nested calls, clearing it on each entry.
- The earlier `Wait None` frame-count mapping was wrong. `None` does not
  select a special wait; as with a bare number, it takes the millisecond
  path. Tests distinguish this from `GameFrame`.
- **Incomplete:** the unnamed flag `0xECAA3345` selects type 14, which needs
  a global script-pass latch. That scheduler is not implemented here.
  Encountering the flag now reports an untranslated `Wait` and blocks
  instead of silently running with invented timing. The research found
  no original script using it.

Tests cover zero/multiple GameFrame counts, separate calls at the same
timestamp, event-driven nested updates, propagation through multiple update
levels, millisecond waits, and explicit reporting of unsupported type 14.

## Evidence and limits

The private research notes at revision
`89b54d299f3a2efb927cac538fb41b35e3c425c3`, sections 14, 15, 18 and 19,
describe immediate dispatch through `82225758` -> `82228628` -> `8220D840`
-> `82224D78`, and the synchronous lip goto/update in `820F44C0`.
These are existing reverse-engineering findings, not a new independent
instruction-level verification. No executable, generated function bodies,
extracted script bytecode or private data dumps are included here.

- **INFERRED:** the regular script update remains after physics. The retail
  component scheduling order is not independently established.
- **UNKNOWN:** how often retail schedules separate top-level updates for
  the skater in one frame. The verified wait rule does not establish that
  component scheduling order.
- Animation waits and scripts spawned by ordinary event handlers remain
  incomplete as described in the handoff.

The event-dispatch checklist item should not be treated as fully verified
while component scheduling and spawned-script lifetime remain unresolved.

## Validation

On Windows, `cargo test --workspace --locked --offline` passes all 93 tests.
The three new skater integration regressions fail with the old deferred
`Skater::step` and pass with synchronous dispatch. The lip test additionally
checks the ordering inside lip setup. Fixtures are synthetic and require no
game data.

The two nested-update regressions also fail with the original VM update
loop and pass with the retail re-entry safeguard.

`cargo clippy --workspace --all-targets --locked --offline` completes with
the two pre-existing `chunks_exact_to_as_chunks` warnings in `qb.rs` and
`vm.rs`. No new Clippy warnings were introduced.

Playable comparison against retail has not been performed.
