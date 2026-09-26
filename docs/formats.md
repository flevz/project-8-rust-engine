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

## `.pak.xen` archives: CONFIRMED

Each file is one raw DEFLATE stream. Inflated, it is a list of 32-byte
big-endian headers (type key, data offset relative to the header, size,
archive key, full-name key, short-name key, parent key, flags), with optional
160-byte names, ending with a `.last` entry. Data may live in a sibling
`.pab.xen`, which is compressed the same way. All 434 archives of a retail
installation parse. See `crates/p8-formats/src/pak.rs`.

## `.qb` scripts: CONFIRMED (globals)

A 28-byte header, then items `u32 type, u32 name key, u32 file key,
u32 value-or-offset, u32 next`. The type's second byte gives the kind:
1 int, 2 float, 3 string, 5 pair, 6 vector, 7 script, 10 struct, 12 array,
13 checksum. Struct members carry the kind in bits 16-22. Script bytecode
is not yet decoded.

## Everything else: UNKNOWN

This covers scenes and levels, collision, textures, models, skeletons,
animations, QB scripts and the node arrays that hold restart points, rails
and goals. Each will be documented here as it is identified.
