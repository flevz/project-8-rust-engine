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

## `.img` textures (Xbox 360): CONFIRMED for DXT5 HUD sprites

Checked on the balance meter's four textures in
`ZONES/global.pak.xen`. `+0x1C` (u32) is the offset of the Direct3D header
(0x28) and `+0x20` the pixel data size; the data is the file's last that
many bytes (both LIKELY: true for every file seen). The Direct3D header is
7 dwords then the 6-dword GPU texture fetch constant (tiled flag, pitch,
format, endian swap, width, height, packed mips). Pixels are tiled with the
standard Xbox 360 tiling, blocks byte-swapped in 16-bit words. A texture 16
texels or less on a side with packed mips starts 16 texels into its tile
(CONFIRMED for a taller-than-wide texture). Only format 0x14 (DXT4/5) is
decoded so far. See `crates/p8-formats/src/texture.rs`.

## `.ske` skeletons: CONFIRMED (bind pose matches the meshes)

See `crates/p8-formats/src/skeleton.rs`: bone count, name / parent / mirror
tables, local positions and rotations; the rotation relative to the parent
is the conjugate of the stored quaternion.

## `.skin.xen` models: CONFIRMED (geometry, skinning, normals, UVs)

One raw DEFLATE stream. Materials (0x134 bytes each, three texture names),
a `0xBABEFACE`-marked geometry section, 0x80-byte mesh records, u16 strip
indices with 0x7FFF restarts, vertices in blocks sharing up to four bones
(32 bytes each: position, two u16 weights, 11:11:10 normal / tangent /
binormal), and a second stream with colour and half-float UVs. See
`crates/p8-formats/src/scene.rs` for the details and what is not read yet.

## `.tex.xen` texture dictionaries: CONFIRMED (colour textures)

`0xFACECAA7`, count, 0x28-byte entries shaped like `.img` headers with a
type byte (0 colour, 1 normal map, 3 other) and the data offset at `+0x24`.
DXT1 and DXT5 decode; DXN normal maps not yet. See `texture.rs`.

## `.ska` animation clips: CONFIRMED (keys match the retail decompressor)

In `PAK/perm_anims.pak.xen`. `+0x0C` header offset, `+0x10` the clip's own
key table, `+0x18` bone mask (bit count, then bits by skeleton bone). Header:
`+4` flags (`0x00800000` compressed keys, `0x00080000` mask used), `+8`
duration, `+0xD` bone count, `+0x18`/`+0x1C` rotation/position data,
`+0x20`/`+0x24` u16 sizes per bone. Compressed rotations are 14-bit fixed
point x, y, z (w rebuilt) with several packings, some through
`DATA/ANIMS/standardkeyQ.bin.xen`; positions are floats. Frames at 60 per
second. See `anim.rs` for every rule and its retail address.

## Everything else: UNKNOWN

This covers scenes and levels, collision, textures, models, skeletons,
animations, QB scripts and the node arrays that hold restart points, rails
and goals. Each will be documented here as it is identified.
