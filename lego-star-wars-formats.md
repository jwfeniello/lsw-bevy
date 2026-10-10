# LEGO Star Wars: The Complete Saga (Android) — what we learned

Everything this project worked out about the game's data: the archive, models, textures,
animations, HUD art and sounds. All of it comes from reading the Android release's data file
(`main.1063.com.wb.lego.tcs.obb`), checked against the open decompilation
[opensagadev/saga](https://github.com/opensagadev/saga) (commit `d6a6d0a8`).

Each section says how sure we are. "Verified" means a tool in `tools/` reads real files with it
and the result was checked (sizes, pictures, or the game running the output). "Not understood"
means exactly that.

All numbers are little-endian. Pointers inside files are 32-bit.

## Contents

1. [The archive (`.obb`)](#1-the-archive-obb)
2. [What is in it](#2-what-is-in-it)
3. [Character definition files (`.txt`)](#3-character-definition-files-txt)
4. [Models (`.ghg`, `.gsc`)](#4-models-ghg-gsc)
5. [Qui-Gon's model in detail](#5-qui-gons-model-in-detail)
6. [Textures (`.atitc`)](#6-textures-atitc)
7. [Animations (`.an3`)](#7-animations-an3)
8. [The lightsaber](#8-the-lightsaber)
9. [HUD art, icons and animated textures](#9-hud-art-icons-and-animated-textures)
10. [Sounds](#10-sounds)
11. [Spaces, units and handedness](#11-spaces-units-and-handedness)
12. [Tools in this repo](#12-tools-in-this-repo)
13. [Open questions](#13-open-questions)

---

## 1. The archive (`.obb`)

**Verified.** Reader: `tools/tt_dat.py`; command line: `tools/obb_unpack.py`.

The `.obb` is not a zip. It is a TT Games `.DAT` archive (the file identifies itself with the
string `PakDat v1.01`), 1,236,862,804 bytes, holding 27,493 files.

### Layout

| Where | Type | Meaning |
|---|---|---|
| byte 0 | `i32` | Offset of the directory. A negative value means `offset * -0x100`. Here: 1,235,500,136 |
| byte 4 | `u32` | Size of the directory (1,362,668) |
| directory + 0 | `i32` | Version. Here **-5** |
| directory + 4 | `i32` | File count (27,493) |
| then | 16 bytes × count | File records: `i32 offset`, `i32 packed_size`, `i32 size`, `i32 mode` |
| then | `i32` | Node count of the name tree |
| then | 12 bytes × nodes | `i16 child`, `i16 sibling`, `i32 name_offset`, `i16`, `i16` (8 bytes per node when version > -5) |
| then | `i32` + bytes | Size of the names blob, then the names: NUL-terminated, Latin-1 |
| then | | A hash table for fast lookup (version < -1). Not needed to read files |

- **File position.** For version < -1 the real position is `offset << 8`: files are aligned to 256
  bytes and the record stores the position divided by 256.
- **Name tree.** Node 0 is the root and its `child` is the first top-level entry. `sibling == 0`
  ends a level. A node whose `child` is zero or negative is a file, and its record index is
  `-child`. Directory names are joined with `\`. Names are compared case-insensitively.

### Compression

`mode` is 0 (stored; 8,003 files) or 3 (19,490 files). Mode 2 (TT's "LZ2K") exists in the format
but never occurs in this archive.

A mode 3 file is a run of blocks:

| Bytes | Meaning |
|---|---|
| 4 | `"DFLT"` |
| 4 | `i32` packed size of this block |
| 4 | `i32` unpacked size of this block (at most 32,768) |
| packed size | The data. If packed size equals unpacked size the block is stored raw |

The block data is **deflate with two changes from RFC 1951**, so `zlib` cannot read it:

1. Block type 0 is dynamic Huffman and type 2 is stored (the RFC has those the other way round).
   Type 1 is fixed Huffman as usual.
2. A stored block is a 16-bit length followed by the bytes, with no complement word.

The Huffman coding itself (code-length alphabet order, length and distance tables) is standard.
`tools/tt_inflate.py` is a small pure-Python inflate for this variant.

## 2. What is in it

Top-level folders and file counts:

| Folder | Files | Contents |
|---|---|---|
| `sharedtextures\` | 13,809 | Every texture, named by hash: `0x3a9b8522.atitc` |
| `chars\` | 6,665 | One folder per character: model, definition, animations |
| `levels\` | 3,799 | Level scenes, terrain, scripts |
| `audio\` | 1,423 | Music (`.ogg`) and sound effects (`.wav`) |
| `builtshaders\` | 953 | Compiled shaders (`.ios_pcode`, `.ios_vcode`) |
| `cut\` | 404 | Cutscenes |
| `stuff\` | 386 | Shared bits: icons, fonts, text, animated-texture scripts, legal screens |
| `scripts\` | 45 | AI and game scripts |

Most common file types:

| Extension | Count | What it is |
|---|---|---|
| `.atitc` | 13,611 | Texture: a DDS file compressed with ATC (section 6) |
| `.an3` | 2,911 | Character animation (section 7) |
| `.bsa` | 2,340 | Blend-shape (face morph) animation that goes with an `.an3` |
| `.txt` | 1,161 | Text: character definitions, level settings, localisation |
| `.wav` | 1,129 | Sound effects, 16-bit PCM |
| `.scp` | 873 | Scripts |
| `.gsc` | 789 | Scene / prop model (same container as `.ghg`) |
| `.ghg` | 344 | Character model (section 4) |
| `.ogg` | 292 | Music |
| `.ats` | 31 | Animated-texture script, plain text (section 9) |
| `.fnt` | 5 | Fonts |

File names carry platform suffixes: `_ios` is the mobile asset set (used on Android too), and
`_lr` on a character model means the low-resolution mobile version.

## 3. Character definition files (`.txt`)

**Read, not formally parsed.** Each character folder has `<name>.txt`, plain text, one setting
per line. `chars\quigonjinn\quigonjinn.txt` (5,954 bytes) starts:

```
name_id=187
icon="quigon_icon"
layers_special=0,1,5
layers_high=0,2,5
layers_medium=0,3,5
layers_low=0,4,5
layers_dead=0,5,6
helmet_locator=9
hair_layer_index=5
radius=0.1
miny=0
maxy=0.42
scale=1.0
tiptoe_speed=0.153867
walk_speed=0.6
run_speed=1.2
head_locator=2
weapon_locator=0
weapon_locator_2=1
jedi
hit_points=4
shadow_locators=4,5
sfx_die="Qui_Death"
sfx_hurt="Qui_Hurt"
streak_1_locators=6,7
streak=green
```

What these tell you:

- **`layers_*`** — which model layers to draw at each detail level (section 4). `layers_special`
  is the close-up set, `layers_dead` the broken-apart set.
- **`maxy`** — the character's height in model units (0.42; the mesh measures 0.412).
- **`*_locator`** — indices of attachment points in the model. `weapon_locator=0` is where the
  saber goes; `streak_1_locators=6,7` are the base and tip of the blade's motion trail.
- **`streak=green`** — his blade colour.
- **`hit_points=4`** — four hearts.
- **`walk_speed`, `run_speed`, `tiptoe_speed`** — movement speeds in model units per second.

After the settings come animation entries:

```
anim_start="walk"
action="walk"
blend_in=0.2
blend_out=0.2
fpsec=37.5
footsteps
frame3=16
frame4=30
anim_end
```

`anim_start` names the `.an3` file in the same folder, `action` is the game action it plays for,
`fpsec` overrides the default 30 frames per second, and `footsteps` with `frameN` gives the
frames footstep sounds fire on. `bsa=on` / `bsa=off` says whether the matching `.bsa` face
animation plays. Other flags seen: `no_headturn`, `cycle=off`, `speed=`, `gun_off`.

## 4. Models (`.ghg`, `.gsc`)

**Verified for geometry, skeleton, layers, locators and materials.** Reader: `tools/ghg_dump.py`
(`Ghg` class and a dump command); converter: `tools/lego_to_gltf.py`. Checked on two files
(Qui-Gon, 300,428 bytes; the Mace Windu saber rig, 212,614 bytes).

A model file is a memory image of the game's own structures, with a pointer table for
relocation. Nothing in it is quantised or GPU-compressed.

### File level

| Offset | Type | Meaning |
|---|---|---|
| 0 | `u32` | `section_size`; the graphics section is `section_size + 4` bytes |
| 4 | `u16` | Texture count. Bit 15 set means the textures are a list of hashes |
| 6 | `u32` × n | Texture hashes (see section 6). Without bit 15 the textures are stored inline, each with a 24-byte header; no file we read uses that |
| … | `u16` | Vertex buffer count, then per buffer: `u32 size` (bit 31 = keep in memory, used for morph targets), then the bytes |
| … | `u16` | Index buffer count, then per buffer: `u32 size`, then `u16` indices |
| `section_size + 4` | | The image |

### The image

- 16-byte header: `"NU20"`, `i32` negative image size, `u32` version (2), `u32` 0.
- Then chunks of `char tag[4]; u32 size`, where size includes the 8-byte header.

Chunk order in Qui-Gon:

| Chunk | Size | Contents |
|---|---|---|
| `HEAD` | 16 | Offset to the pointer table, pointer to the scene header |
| `NTBL` | 704 | Name table: `u32` byte count, then NUL-terminated strings from chunk + 0x0C |
| `MS00` | 5,680 | Materials: 8 records of 0x2C4 bytes |
| `INID` | 16 | Empty |
| `FDNS` | 208 | Not understood (the display scene's fade ranges point here) |
| `BNDS` | 784 | Not understood (24 bounds records of 0x20 bytes) |
| `TAS0` | 64 | Not understood (one texture-animation record) |
| `CDAT` | 49,872 | The scene data: joints, matrices, locators, layers, display scene, geometry records |
| `TREF` | 16 | Empty |
| `TST0` | 416 | Texture records: 9 × 0x28 bytes (width, height, 16-byte checksum) |
| `VBIB` | 32 | Buffer handle arrays, filled at run time |
| (untagged) | 32 | 0x20 bytes with no tag, at image offset 0xE1E0 |
| `SALI` | 16 | Not understood |
| `DYNO` | 128 | Not understood |
| `GSNH` | 848 | The scene header (0x1F8 bytes) and its pointer arrays |
| `PNTR` | 10,656 | The pointer table: 2,660 entries |

**Pointer table (`PNTR`).** `u32 count`, then `count × u32`. Each entry is the address of a
pointer, relative to the entry itself. Each pointer holds a value relative to its own address;
zero stays null. Relocating means turning both into image offsets.

### Scene header (`GSNH` + 8, 0x1F8 bytes)

| Offset | Meaning |
|---|---|
| 0x04, 0x08 | Texture count; pointer to an array of pointers to texture records |
| 0x0C, 0x10 | Pointer to an array of material pointers; material count |
| 0x110 | Pointer to the display scene (0x90 bytes) |
| 0x168 | Joint count |
| 0x16C | Joints, 0x60 bytes each |
| 0x170 | Local bind matrices, 0x40 each |
| 0x174 | Inverse bind matrices, 0x40 each |
| 0x180, 0x184 | Locator record count; records of 0x50 bytes |
| 0x188, 0x18C | Locator map count; map bytes (game locator N is record `map[N]`; 0xFF = absent) |
| 0x190, 0x194 | Layer count; layers of 0x14 bytes |
| 0x1AC, 0x1B8 | Bounds minimum and maximum (3 floats each) |
| 0x1D0 | Native scene: geometry pointer array at +0x10, geometry count at +0x14 |

### Joints (0x60 bytes)

- A 4×4 matrix at 0. This is the joint's **bind rotation**, which the animation system needs
  (section 7). For most joints it equals the rotation of the local bind matrix; for 12 of
  Qui-Gon's (elbows, `ponyTailOne`, …) it differs.
- Name pointer at +0x4C, parent index (`u8`, 0xFF = root) at +0x50, a flag byte at +0x51.
- Parents always come before their children.

**Matrix convention.** Row-major with row vectors: `v_model = v_joint @ M`, translation in the
fourth row. The rest pose of joint j is `local[j] @ world[parent]`. Check: `inverse_bind @ world`
is the identity to 1e-7 in both files.

### Locators (0x50 bytes)

Attachment points: a 4×4 matrix relative to a joint, a name at +0x40 and the joint index (`u8`)
at +0x44. The name field relocates to `0xED5C0000 + offset`; its low 16 bits are a valid offset
into the name table. Why the high half is `0xED5C` is not understood, so the names are read
with that heuristic. Joints and matrices do not depend on it.

### Layers (0x14 bytes)

A layer is a named set of pieces that are drawn or hidden together. Each record has: name
pointer, pointer to a per-joint table of **rigid** piece handles, pointer to a **skinned** piece
handle, then the same pair again ("alternate"). The character `.txt` picks which layers to draw.

### Display scene, pieces and geometry

- **Display scene** (0x90): item count at 4, items at 8 (16 bytes each: `u8 type`, `u8 id`,
  `i16`, pointer), special count at 0x6C, specials at 0x70.
- **Display special** (0xD0): one drawable piece. Two matrices, bounds at 0x80/0x90, clip-object
  pointer at 0xB0, name at 0xB4.
- **Clip object** (12 bytes): count, pointer to material indices, pointer to item indices. Draw
  k of a piece uses material `ids[k]` and item `indices[k]`. An item of type 0x82 points at a
  geometry. Type 0x8F items are face-on sprites; type 0x83 items point at 0x40-byte packets we
  did not decode.
- **Geometry** (0x38):

| Offset | Type | Meaning |
|---|---|---|
| 0x00 | `u32` | Primitive type. 6 = indexed triangle strip, the only one seen |
| 0x04 | `i32` | Primitive count; the strip has count + 2 indices |
| 0x08 | `u16` | Vertex stride |
| 0x0A | `u8 × 8` | Joint palette, 0xFF-terminated. Empty for rigid pieces |
| 0x14 | `i32` | Base vertex (0 everywhere) |
| 0x18 | `i32` | Vertex count |
| 0x1C | `i32` | First index, in `u16` units |
| 0x20 | `u32` | Index buffer number |
| 0x24 | `u32` | Vertex buffer number |
| 0x28 | `i32` | Non-zero for morph-target geometry |
| 0x2C | ptr | Morph-target offset arrays |

Indices are absolute into the shared vertex buffer, and each geometry owns one contiguous run of
vertices. Strips alternate winding: flip every odd triangle and drop degenerate ones.

### Vertex layouts

The layout comes from a 32-bit descriptor in the material at +0x1F0. The rule is in
`ghg_dump.vertex_layout`. Layouts seen:

| Descriptor | Stride | Layout |
|---|---|---|
| 0x02000929 | 36 | position `f32×3` @0, normal `u8×4` @12, tangent @16, binormal @20, colour @24, uv `f32×2` @28 |
| 0x02028929 | 44 | the same, plus weights `u8×4` @36, joints `u8×4` @40 |
| 0x02428909 | 40 | position, normal @12, binormal @16, colour @20, uv @24, weights @32, joints @36 |
| 0x00000109 / 0x00000909 / 0x00028109 / 0x00028909 | 20 / 28 / 28 / 36 | position, normal, colour, optional uv, optional weights + joints (saber file) |

- **Normals:** bytes; `n = byte / 255 * 2 - 1`. The fourth byte is unused.
- **Skin:** three byte weights and three palette slots; the fourth byte of each is 255. The joint
  is `geometry.palette[slot]`. Qui-Gon's weights sum to exactly 255. In the Mace Windu file some
  sum to 254 or 253, so normalise.
- **Rigid pieces** are stored in their joint's space: multiply by that joint's rest matrix.
  **Skinned pieces** are already in model space.
- **UVs:** V runs from -1 to 0 on Qui-Gon and 0 to 1 on the saber hilt. Using `v mod 1` with
  the image's top row at v = 0 gives the right picture (confirmed in game).

### Materials (0x2C4 bytes)

Fields we read: diffuse texture indices `i32×4` at +0xB8, specular at +0xFC, normal map at
+0x100, colour `f32×3` at +0x54, opacity at +0x70, vertex descriptor at +0x1F0. Most of the
record is not understood.

### `.gsc` files

Scene and prop models use the same container. The texture hash list at the start of a `.gsc`
reads with the same code, which is how the HUD icon textures were found (section 9). We did not
decode `.gsc` geometry.

## 5. Qui-Gon's model in detail

`chars\quigonjinn\quigonjinn_lr_ios.ghg`: 42 pieces, 5,278 vertices, 5,904 triangles, 0.412
tall, feet at y = 0, **facing -z**.

### Joints (34)

| Index | Name | Parent | | Index | Name | Parent |
|---|---|---|---|---|---|---|
| 0 | character | – | | 17 | rightElbow | 16 |
| 1 | upperTorso (the hips) | 0 | | 18 | rightElbowLength | 17 |
| 2 | body | 1 | | 19 | rightHand | 18 |
| 3 | chest | 2 | | 20 | weaponRight | 19 |
| 4 | head | 3 | | 21–25 | cloak0 … cloak4 | chain from 3 |
| 5 | helmet | 4 | | 26 | rightLeg | 0 |
| 6–8 | ponyTailOne/Two/Three | chain from 5 | | 27 | rightKnee | 26 |
| 9 | leftArm (clavicle) | 3 | | 28 | rightAnkle | 27 |
| 10 | leftShoulder | 9 | | 29 | rightToe | 28 |
| 11 | leftElbow | 10 | | 30 | leftLeg | 0 |
| 12 | leftElbowLength | 11 | | 31 | leftKnee | 30 |
| 13 | leftHand | 12 | | 32 | leftAnkle | 31 |
| 14 | weaponLeft | 13 | | 33 | leftToe | 32 |
| 15 | rightArm (clavicle) | 3 | | | | |
| 16 | rightShoulder | 15 | | | | |

Rest positions that matter (model units): hips (0, 0.1067), chest (0, 0.2044), head (0, 0.2696),
shoulders (±0.0609, 0.2458), elbows (±0.0909, 0.1964), hands (±0.0955, 0.1622), leg pivots
(±0.0399, 0.1067), knees y 0.0582, ankles y 0.0212. His "left" joints are at +x.

### Locators (game order)

| # | Name | Joint | Use |
|---|---|---|---|
| 0 | `weaponRight\|LightSabre` | weaponRight | Where the saber attaches (`weapon_locator=0`) |
| 1 | `weaponLeft\|LightSabre` | weaponLeft | Second weapon |
| 2 | `poi` | head | `head_locator` |
| 3 | `poi1` | chest | |
| 4, 5 | `poi2`, `poi3` | rightAnkle, leftAnkle | `shadow_locators` |
| 6, 7 | `blur`, `blur1` | weaponRight | Base and tip of the blade (`streak_1_locators`) |
| 8 | `locator3` | head | `tag_locator` |
| 9 | `locator4` | head | `helmet_locator` |

### Layers

| # | Name | Contents |
|---|---|---|
| 0 | `TT0_AlwaysDrawn` | Two small skinned face pieces (eyes, brows, mouth print), with morph targets |
| 1 | `TT1_HighRes` | The body at full detail |
| 2 | `TT2_MediumRes` | Rigid `upperTorso` (hips), `chest` (neck stud), `head`; skinned `skin.0` … `skin.4` |
| 3 | `TT3_LoRes` | Lower detail; hands become rigid pieces |
| 4 | `TT4_VeryLoRes` | Lowest detail |
| 5 | `TT5_Hair` | Two skinned hair pieces |
| 6 | `TT6_Breakup_Lastgen` | Rigid `leftArm`, `rightArm`, `head`, `upperTorso`, for when he falls apart |

The game draws layers 0, 2, 5 in normal play; this project uses 0, 1, 5.

### The skinned pieces are not LEGO parts

This cost us a wrong write-up, so it gets its own heading. Skinned pieces are **draw batches cut
at the joint-palette limit**, grouped by material. Their boundaries do not follow LEGO parts.
Measured by each vertex's strongest joint, layer 2:

| Piece | Actually contains |
|---|---|
| `skin.0` | weaponRight 42, rightHand 34, both elbows 8 each, leftHand 8 |
| `skin.1` | **leftHand 70**, chest 18, cloak joints 30 |
| `skin.2` | The right arm: rightShoulder 108, rightElbow 36, rightElbowLength 25 |
| `skin.3` | leftShoulder 108, chest 80, body 40, leftElbow 36, **and the right foot** (rightToe 12, rightAnkle 8) |
| `skin.4` | Both legs, without the right toe |

286 of the 848 skinned vertices in that layer have more than one influence. Finger vertices bind
to `weaponRight` / `weaponLeft`. To split him into rigid LEGO parts, assign each vertex to its
strongest joint and fold the weapon joints into the hands.

## 6. Textures (`.atitc`)

**Verified.** Decoder: `tools/atc.py`.

- Models refer to textures by 32-bit hash. The file is `sharedtextures\0x<hash>.atitc`
  (lower-case hex, eight digits).
- Each `.atitc` is a standard **DDS** file: `"DDS "`, a 124-byte header (height at byte 12, width
  at 16, FourCC at 84), then a full mip chain.
- The FourCC is `ATC ` or `ATCA`. ATC is a mobile-GPU (Adreno) block format related to DXT.
  Pillow cannot open it.
- The scene header claims twice the real size (says 1024×1024, the file is 512×512).

### ATC colour block (8 bytes per 4×4 pixels)

| Bytes | Meaning |
|---|---|
| 0–1 | `u16` colour 0: bit 15 = alternate mode, then 5 bits each of red, green, blue |
| 2–3 | `u16` colour 1: 5 bits red, 6 green, 5 blue |
| 4–7 | `u32` of sixteen 2-bit indices, row by row from the low bits |

- **Normal mode** palette: `c0`, `(2·c0 + c1) / 3`, `(c0 + 2·c1) / 3`, `c1`.
- **Alternate mode** palette: black, `c0 − c1/4`, `c0`, `c1`.

### `ATCA` (16 bytes per block)

Eight bytes of alpha, then the colour block. **The alpha is interpolated, DXT5-style** (two
endpoint bytes, then sixteen 3-bit indices), not the 4-bit explicit alpha the name suggests.
Decoding it as explicit alpha gives striped noise; that mistake was made here once.

### Qui-Gon's textures

| File | Size | Format | Use |
|---|---|---|---|
| `0x3a9b8522` | 512×512 | `ATC ` | Body, head, limbs, cloak: tunic print, belt, beard, plain colours |
| `0xbe9f904e` | 512×512 | `ATCA` | Normal map for the above |
| `0xf13b91c7` | 256×256 | `ATCA` | Face print overlay (brows, eyes, mouth, moustache), with alpha |
| `0xde73adc5`, `0xbb4b8741` | 64×64 | `ATC ` | Hair |
| `0xfba4b214`, `0x6e7f1230` | 64×64 | `ATCA` | Hair normal maps |
| `0x48291ff3`, `0xfebbf7c6` | 64×64, 256×256 | | Listed by the model but not used by any material slot we read |

## 7. Animations (`.an3`)

**Verified** for the channel types Qui-Gon's files use (which is all 66 of his files). Decoder:
`tools/an3.py`. Walk, run and idle were rendered and then played in another game through this
decoder.

Magic bytes `34 49 4E 41` (`"ANI4"` read as a little-endian number, 0x414E4934).

### Header

| Offset | Type | Field | walk.an3 |
|---|---|---|---|
| 0x00 | `u32` | Magic | |
| 0x04 | `u16` | Node count | 34 |
| 0x06 | `u16` | Key count (samples per keyed curve) | 15 |
| 0x08 | `u16` | Key stride = 4 × number of keyed curves | 236 |
| 0x0A | `u16` | Frame count | 30 |
| 0x0C | `u16` | Curves per node | 9 |
| 0x0E | `u16` | First frame | 0 |
| 0x12 | `u8, u8` | Unknown; format flags (bit 0 quaternion, bit 1 stores w) | 0, 0 |
| 0x14 | `u16` | Offset of a chained header | 0 |
| 0x1C | `f32, f32` | Minimum and scale for constant curves | |
| 0x24 | `u32` | Offset of the per-curve (scale, minimum) float pairs | 0x484 |
| 0x28 | `u32` | Offset of the `u16` constant pool | 0x5C |
| 0x2C | `u32` | Offset of the `u16` curve types, node count × 9 | 0xEC |
| 0x30 | `u32` | Offset of the key blocks | 0x65C |
| 0x34 | `u32` | Offset of the node flag bytes | 0xAF8 |
| 0x3C | `char[32]` | Name | `"Walk"` |

### Nodes and channels

- One flag byte and nine curve types per node. The nine slots are translation xyz, rotation xyz,
  scale xyz.
- **Flag bits:** 0x01 has rotation, 0x02 has translation, 0x08 has scale, 0x10 cancel the
  parent's scale, 0x20 multiply by the joint's bind rotation. A group whose bit is clear reads
  as 0 / 0 / 1 and owns no data.
- **Curve type ≥ 16** is a constant: `pool[type − 16] × scale + minimum`, using the header's
  values.
- **Curve type 6** is a keyed curve. These are the only two kinds in his files (3,792 keyed
  curves, all type 6). The game also has types 7 and 9 and quaternion formats; we did not
  implement them.
- **Node i drives joint i.** There is no name table; the counts simply match (34 and 34).

### Key encoding (type 6)

Keys are stored in blocks of four. A block is `key stride` bytes: one `u32` per keyed curve, in
stream order (nodes in order; within a node, translation, rotation, scale; keyed slots only).

- The low byte is a base value. The upper 24 bits are four 6-bit fractions.
- Key `4b + q` = `base[b] + (base[b+1] − base[b]) × fraction[b][q] / 63`, then
  `× curve_scale + curve_minimum`.
- So there is one more block than the keys need (walk: 15 keys, 5 blocks).

### Evaluating a frame

- `key = (frame − first_frame) × (key_count − 1) / (frame_count − 1)`, clamped, then linear
  interpolation between neighbouring keys. `frame` is zero-based.
- **Translation** is in model units, relative to the parent.
- **Rotation** is Euler angles in radians, X then Y then Z, in row-vector form.
- **Local matrix:** `R = RotXYZ(angles)`; with flag 0x20, `R = R @ bind_rotation` (the 4×4 at
  the start of the joint record, section 4); add the translation in row 3; then mirror in z
  (`diag(1,1,−1) @ M @ diag(1,1,−1)`), because the game composes in a z-negated space.
- **World matrix:** `local @ world[parent]`, the same chaining as the model's rest pose.

### Timing and looping

- The frame rate is not in the file. The default is 30 frames per second; an `fpsec=` line in
  the character `.txt` overrides it.
- A looping animation wraps by `frame_count − 1` frames: the last frame equals frame 0.

| Animation | Frames | Keys | Rate | Loop | Vertical bob |
|---|---|---|---|---|---|
| walk | 30 | 15 | 37.5 | 0.773 s | 0.015 |
| run | 30 | 15 | 40 | 0.725 s | 0.056 |
| tiptoe | 45 | 22 | 30 | 1.467 s | 0.020 |
| idle | 180 | 90 | 30 | 5.967 s | 0.009 |
| sabreidle | 106 | 53 | 30 | 3.500 s | 0.008 |

### Things worth knowing

- **There is no root motion.** Walk, run and tiptoe play in place: the root joint (`character`,
  the only node with keyed translation) bobs and sways but does not travel. The game moves the
  character itself.
- **Limbs are rigid.** Bone lengths hold to 0.1% in every frame, and no joint has scale. A
  minifig's arm swings at the shoulder and its leg at the hip.
- **Idle is not still.** The first half is nearly motionless; then he turns about a quarter
  turn to one side with a step, leans back the other way, and returns.
- **Foot speed.** Tiptoe's planted foot moves at 0.155–0.159 units/s, matching
  `tiptoe_speed=0.153867`. Walk's moves at 0.36–0.38, which does not match `walk_speed=0.6`;
  LEGO characters slide a little when they walk.
- **`.bsa` files** are face (blend-shape) animations in an older curve format. They load into a
  separate slot and only play when the animation entry says `bsa=on`. They do not affect the
  skeleton.
- Qui-Gon's folder has animations named for every action: `walk`, `run`, `tiptoe`, `idle`,
  `sabreidle`, `lightsabrein`, `lightsabreout`, `backflip`, `block1`–`3`, `combo1_*`,
  `combo2_*`, `lunge`, `buildit`, and more.

## 8. The lightsaber

**Verified** (drawn in his hand in another game, lining up with the blade locators).

There is no standalone saber model in the obvious place. `chars\macewindu\lightsabre_lr_ios.ghg`
is a **full Mace Windu character rig** (31 joints, five layers) with one extra root joint named
`|LightSabre`, and the saber hangs off that joint as two rigid pieces in layer 0:

| Piece | Vertices | Triangles | Notes |
|---|---|---|---|
| Hilt | 450 | 376 | 0.142 long. Textured with `sharedtextures\0xe84b1735.atitc` (64×128) |
| Blade | 34 | 24 | A thin rod 0.278 long, radius about 0.011. Untextured. Material colour (0.853, 0.891, 1.0), opacity 0.975 |

- The blade in the file is a pale core rod. The colour and glow are added by the game (the
  character's `streak=` setting); they are not in the model.
- The character in that file is stored upside-down (its root joint's rest matrix is a 180°
  turn). Only the two saber pieces are useful.
- **Placing it in a hand:** undo the `|LightSabre` joint's rest matrix to get the saber's own
  space, then apply the holder's weapon locator matrix and that locator's joint matrix:
  `v_model = v_saber @ locator_matrix @ world[locator_joint]`. Done that way, the blade runs
  exactly between Qui-Gon's locators 6 and 7.
- `stuff\sabre_red_ios.gsc` also exists (8,930 bytes); not decoded.
- Saber animations exist per character: `sabreidle`, `lightsabrein`, `lightsabreout`.

## 9. HUD art, icons and animated textures

### Character icons

**Verified.** `stuff\icons\<name>_icon_ios.gsc` is a tiny scene whose texture list names the
portrait. For Qui-Gon (`icon="quigon_icon"` in his `.txt`): hash `0x52a8b68a`, a 256×256 `ATCA`
picture of his head with transparency. The coloured disc behind portraits in the HUD is not in
the icon; the game draws it.

### Other small HUD textures

`stuff\pc_bits_ios.gsc` lists four 32×32 textures (`0x81eab61d`, `0x3362d8fb`, `0x26e5cb31`,
`0x350fd45a`): plain green, yellow, red and blue discs, the controller button colours.
`stuff\ps2_bits_ios.gsc` lists `0x42406f91`, a 256×256 Death Star picture.

### Hearts and studs are not pictures

We did not find heart or stud sprites as textures. In this game they are drawn as models or
animated textures. The HUD in this project uses sprite rips from the PC release instead (kept in
`work/hud`, not in git):

- A HUD items sheet with the red heart (32×32), the grey heart, the True Jedi bar and portrait
  discs.
- A stud sheet: 32 frames of the silver, gold, blue and purple studs spinning, 65×65 each, plus
  the 10-step stud bar.

### Animated-texture scripts (`.ats`)

**Read.** Plain text. `stuff\ats\coin.ats` is the stud's spin:

```
Scriptname "coin"
;ScriptMask 3

Start:
	Rate	2,0   ;;update rate plus rand
	wait 0,30
coin:
	Repeat
	TexAdj 1,0,31
	UntilTex = 31
	Tex 0
	Goto 	coin
```

It steps through textures 0 to 31 and loops, which is the 32-frame spin in the PC sprite sheet.
`coin_pause.ats` is the variant for the pause screen. Other scripts drive holograms, screens,
lightning and blinking.

### Fonts

`stuff\text\starwars_font_ios.fnt` (2.8 MB), `starwars_font_pc.fnt`, and button-prompt fonts.
Not decoded; the HUD's stud count in this project uses a system font.

## 10. Sounds

**Used directly.** Sound effects are ordinary RIFF/WAVE files, 16-bit PCM, typically 22,050 Hz
mono. Music is Ogg Vorbis under `audio\_music\`.

| Path | What |
|---|---|
| `audio\_soundfx\saber\` | 25 saber sounds: `saberonj.wav` (ignite), `saberoffj.wav`, `saberloopj.wav` and `saberloopb.wav` (hum), `sabermove1`–`4` (swings), `sabersaber01`–`04` (clashes), `sabersmack*`, `sabdefblas1`–`4` (deflecting blaster bolts), `combosaber1`–`2` |
| `audio\_soundfx\quigon\` | `qui_death.wav`, `qui_hurt1.wav`, `qui_hurt2.wav` |
| `audio\_soundfx\picknew\` | Pickups: `coindrops.wav` (silver stud, 0.15 s), `coindropg.wav` (gold stud, 0.31 s), `powerup_on/off/loop`, `hiddenitemreveal`, `victory1`, `status_*` |
| `audio\_soundfx\_status\` | HUD sounds such as `char_icon_app.wav`, `char_icon_slide.wav` |

The character `.txt` refers to sounds by name (`sfx_die="Qui_Death"`), not by path.

## 11. Spaces, units and handedness

- **Up is +y**, the floor is y = 0, and a minifig is about 0.41 units tall. Speeds in the
  character files are in these units per second.
- **Qui-Gon faces −z**, with his left side at +x. That is a mirrored (left-handed) arrangement
  compared with a +y-up, right-handed space where a character facing −z has his left at −x.
- To put him in a right-handed space facing +z with his left at +x, **mirror z**. Rotating by
  180° instead puts his left arm on the wrong side. Mirroring flips triangle winding, so
  re-derive the winding from the normals afterwards.
- A row-vector rotation `M` from the files becomes a column-vector rotation in the mirrored
  space as `F @ Mᵀ @ F` with `F = diag(1, 1, −1)`.
- The animation system works in a z-negated space internally (section 7), which is why animated
  local matrices are mirrored before chaining.

## 12. Tools in this repo

| Tool | What it does |
|---|---|
| `tools/tt_inflate.py` | Inflate for TT's deflate variant |
| `tools/tt_dat.py` | Read the archive: list, find by wildcard, read a file |
| `tools/obb_unpack.py` | Command line: `list ARCHIVE [PATTERN]`, `extract ARCHIVE OUT_DIR PATTERN...` |
| `tools/ghg_dump.py` | Parse a model; `python tools/ghg_dump.py FILE.ghg` prints every chunk and structure |
| `tools/lego_to_gltf.py` | `load()` / `load_pieces()` return the pieces (with joints and weights); writes glTF |
| `tools/atc.py` | `decode_dds(path)` → RGBA array from an `.atitc` |
| `tools/an3.py` | `load()`, `pose(ghg, animation, frame)`, `skin(...)`; `--sheet` writes a contact sheet |
| `tools/preview.py` | Render a mesh to a PNG (front and side) |

Examples:

```
python tools/obb_unpack.py list main.1063.com.wb.lego.tcs.obb "chars/quigonjinn/*"
python tools/obb_unpack.py extract main.1063.com.wb.lego.tcs.obb work/lego "chars/yoda/*"
python tools/ghg_dump.py work/lego/chars/quigonjinn/quigonjinn_lr_ios.ghg
python tools/an3.py work/lego/chars/quigonjinn/walk.an3 --sheet walk.png --model work/lego/chars/quigonjinn/quigonjinn_lr_ios.ghg
```

Only Qui-Gon and the saber rig have been run through the model and animation tools. Other
characters use the same formats, but may hit vertex layouts or animation channel types we have
not seen; the tools raise an error in that case and do not guess.

## 13. Open questions

- **Model chunks walked but not understood:** `FDNS`, `BNDS`, `TAS0`, `SALI`, `DYNO`, the
  untagged 0x20 block, and about 26 KB of `CDAT` (LOD ranges, visibility flags, sort priorities,
  per-material display lists, the type-0x83 packets).
- **Most of the 0x2C4-byte material record.** One descriptor bit (0x400000, "instanced" in the
  decompilation) is ignored; strides still match everywhere.
- **Locator names:** why the name field's high half is `0xED5C`.
- **Morph targets** (the face pieces) are read as their base shape only; `.bsa` face animation
  is not decoded.
- **Animation:** key types 7 and 9, quaternion formats and chained blocks are not implemented.
- **`walk_speed` versus the walk animation's foot speed** (0.6 against a measured 0.37).
- **`.gsc` geometry, `.fnt` fonts, levels, cutscenes, scripts and shaders** are untouched.
- **Where the stud and heart art lives** in the mobile data, if anywhere as pictures.
- **Inline textures** in a model's graphics section: implemented from the decompilation but no
  file we read uses them, so untested.
- **LZ2K** (archive mode 2): not implemented, because this archive never uses it.
