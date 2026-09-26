# Project 8 file formats

Confidence: **CONFIRMED** (verified against the retail game or its code),
**LIKELY** (strong inference, not yet verified on Project 8 files) or
**UNKNOWN**.

## QB checksums: CONFIRMED

The engine names everything by a 32-bit "QB key": CRC-32 with the reflected
polynomial 0xEDB88320 and initial value 0xFFFFFFFF, over the lower-cased
name with `/` replaced by `\`, and **no final complement**.
`p8_formats::qb_key` reproduces five retail pairs quoted by Project8Recomp
(for example `level` = 0x651533EC).

## `.pak.xen` archives: container CONFIRMED, layout LIKELY

A list of 32-byte big-endian headers (type key, relative data offset, size,
archive key, full-name key, short-name key, parent key, flags), with optional
160-byte names, ending with a `.last` entry. Data may live in a sibling
`.pab.xen`. See `crates/p8-formats/src/pak.rs`. **Next step:** run
`p8-inspect` on a real installation to confirm or correct this.

## Everything else: UNKNOWN

This covers scenes and levels, collision, textures, models, skeletons,
animations, QB scripts and the node arrays that hold restart points, rails
and goals. Each will be documented here as it is identified.
