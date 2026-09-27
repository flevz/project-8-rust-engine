# Lip balance display

CONFIRMED (implementation): the translated skater HUD displays a vertical
balance gauge while a lip trick's meter is active. Its position is the current
lip lean divided by the same stat-scaled `Lean_Bail_Angle` used by the balance
update, including custom parameters. Positive lean points toward the top,
matching `OffMeterTop`. Leaving the lip state or stopping the meter hides it.

The display reads existing balance state; it does not advance balance timing,
consume random numbers, or change controls or failure thresholds.

TEMPORARY (presentation): gauge dimensions, placement, colors, warning color
above 75% lean, and text are original placeholder UI choices. They are not a
reconstruction of the retail HUD.

Validation: synthetic tests cover default and custom limits, inactive meters,
both needle directions, hiding on exit, and unchanged balance/RNG state.
An offscreen Bevy render of the actual gauge was inspected for layout.

Spine-transfer physics is a separate pending change. The original function
analysis is still needed; this change preserves the existing vert-break
fallback.
