# Design notes

How the project works, and where each rule came from. "Decomp" means the opensagadev/saga
decompilation, read as a reference; file names are given so a rule can be re-checked. "Notes"
means `lego-star-wars-formats.md`. "Estimated" means this project chose it.

## Shape

Three crates, so the slow-to-compile engine sits on top of small testable parts:

1. `tt_formats` reads the game data. No dependencies.
2. `lsw_sim` holds every gameplay rule as plain Rust with plain numbers. No renderer.
3. `lsw` is the Bevy app: input in, `lsw_sim` stepped once per frame, result drawn.

Bevy's entity system is used only for drawing. The character is one small state struct, which
keeps the per-frame cost flat and the rules testable.

## Spaces

- Game space: +y up, left-handed. Headings are 16-bit angles, 0 = +z, 0x4000 = +x
  (decomp `nuvec.cpp`: rotating (0,0,1) by an angle gives (sin, 0, cos)).
- The model faces -z in its own space and heading 0 walks towards +z, so it is drawn with an
  extra half turn.
- Renderer space is the game space with z negated. Meshes are mirrored in z on the CPU and
  triangle winding is fixed once at load by comparing with the stored normals.
- Checked in screenshots: running right faces right, with the right hand on the camera side.

## Data loading (`tt_formats`)

Follows the notes. Details found by reading the real file that the notes do not spell out:

- A layer record is name, then a main set and an alternate set; each set is a pointer to a
  per-joint table of rigid handles and a pointer to one skinned handle. A handle is 12 bytes
  with the piece's address at +8. Layer 0's face pieces are in the alternate set.
- A piece's draw list stores material and item indices as 32-bit values.
- Rigid piece vertices are already in their joint's space (so the saber piece on the
  `|LightSabre` joint is in saber space as stored).
- Vertex layout is derived from the stride and whether the geometry has a joint palette.
- Row-vector rotations use the engine's convention (decomp `numtx.cpp`).

`ttdump ... check chars\quigonjinn quigonjinn_lr_ios.ghg idle 0` parses all 66 animations and
prints posed joints next to rest joints; feet land at y = 0.

Drawing is CPU skinning into ordinary meshes each frame (about 1,400 vertices), chosen because
it has one code path for rigid pieces, skinned pieces and the saber, and can be checked with
numbers.

## Character rules (`lsw_sim::player`)

Step order: stick, gravity, jump rules, weapon, hold, combo, fall-landing, directional
movement, integrate, animate.

| Rule | Value | Source |
|---|---|---|
| Stick dead zone | radius 42.5/127.5 | decomp `move.cpp` MovePlayer |
| Speed tiers | <0.2 none, <0.5 tiptoe, <0.8 walk, else run | same |
| Stick is relative to camera yaw | | same |
| Velocity | closes `acceleration * dt` of the gap to target | MovePlayer_DIRECTIONAL |
| Turning | heading closes `8 * turn_rate * dt` of the gap | same |
| Reversal (>150 degrees) delays the turn 0.2 s | | same |
| Character defaults (gravity -5, jump 2, accel 5, ...) | | decomp `character.cpp` defaults |
| Qui-Gon: gravity -6, jump 2.3, second jump 3, accel 10 | | his character file |
| Gravity in lunge -10, in slam -15 | | ApplyGravity |
| Jump is "landed" only after 0.1 s | | jump code in `move.cpp` |
| Re-jump within 0.2 s of landing uses second jump speed | | same |
| Double jump only while falling slower than 1.25; raises speed to reach the second jump's height | | same |
| Attack in a single jump = lunge; after a double jump or flip = slam (up-speed 3) | | same, `jumping.cpp` |
| Jump inside the reversal delay = backflip, travelling along the stick without turning | | same |
| Lunge travels at the lunge clip's `speed=` (1.5) | | StartLunge |
| Combo: travel at the clip's `speed=` until frame2, then stop | | MovePlayer_DIRECTIONAL case 5 |
| Combo: 0.2 s window after a swing; press in the last 0.1 s is queued; an early press loses the "b" branch | | LightSabreComboCode |
| Combo chains alternate, chain 2 first; steps are base+1/+2, then +3/+4 or +5/+6 | | same |
| Hit on `frame1`, or at the end of the swing | | same |
| Slam landing kicks the camera 0.5 | | jump code |
| Slow saber draw: blade grows between the clip's frame1 and frame2 | | `playeritems.cpp` |
| Quick saber draw: 5 per second | | FastWeaponOut |
| Clip defaults: 30 fps, 0.2 s blends, looping | | `charconfig.cpp` |

Estimated or simplified:

- Which clip plays for idle / tiptoe / walk / run / fall, and switching to the fall clip at the
  end of a jump clip.
- Putting the saber away on a button press.
- Hard landings other than the slam kick the camera -0.2 (the game does this for some landings;
  not confirmed for the lunge).
- Left out: the turn-around clip, block, swipe-behind, hold behaviour beyond "wait for release",
  water, slopes, moving platforms, lean, the faster-fall rule, touch-control special cases.
- The lunge height uses the character's height (0.42); the file's `lunge_jumpspeed=0.75` is
  not used because its meaning was not traced.

### Added after tracing the animation and player-update code

| Rule | Source |
|---|---|
| Clip tiers split halfway between tiptoe/walk and walk/run speeds | decomp `gameanim.cpp` MoveAnim_Manage |
| Leaving the run clip, and leaving idle, each wait 0.1 s | MoveAnim_Check |
| A jump clip that has been finished 0.1 s with the floor more than 0.35 below switches to the fall clip; from then no lunge or slam | JumpAnimCode, jump code |
| Fall clip in the air, except for 0.2 s after leaving the ground or within 0.25 of the floor | Animate_JEDI |
| Landing after 0.2 s or more of the fall clip uses the heavier landing clip | jump code |
| A jump is still allowed for 0.2 s after leaving the ground | decomp `players.cpp` |
| Event frames in character files count from 1 | StartAnimation |
| The saber is put away by the special button | LightSabreComboCode (the other trigger bit is only set by AI code) |

Still unconfirmed: `lunge_jumpspeed` (not parsed by the reference code), the air-steering flag,
and the exact order of steps inside one frame.

## Levels

A level folder holds a scene (`<name>_ios.gsc`), collision (`<name>.ter`) and settings
(`<name>.txt`). All found by reading the real files against the decomp.

**Scene.** Same container as a character model. Placed objects are listed in the display
scene (count at +0x10, 12-byte records at +0x14: draw count, material ids, item ids). Each item
is a geometry record; the item just before it holds its 4x4 placement matrix.

- Vertex layout comes from the material's descriptor bits (decomp
  `nuvertexformat_android.cpp`): position, normal 0x8, tangent 0x20, binormal 0x2000000,
  colour 0x100, second colour 0x600, texture coordinates in bits 11-13, weights 0x8000,
  joints 0x20000, and 8 extra bytes when 0x4000000 is set.
- Level files do **not** store the tangent and binormal streams even though the descriptor and
  the geometry record's stride say so. The reader measures the real stride (buffer size divided
  by the number of vertices the indices reach) and drops those streams when needed.
- Vertex colours carry the baked lighting, with 0x7F as full brightness.
- Splines are 12-byte records (point count, point size, name, points): camera rails, the
  `start` point, door lines.

**Collision.** A `u32` gives half the offset of a table of 0x34-byte chunk records; chunk data
runs from byte 4. Chunk types 0 (fixed) and 1 (belongs to a scene object) are groups of
0x64-byte shapes: bounds, four points relative to the group origin, two normals, surface type.
The four points are a strip: triangles 0,1,2 and 1,2,3. Type 2 chunks are **wall lines**:
polylines in x and z that cannot be crossed at any height (the room edges and furniture are
fenced this way, not with wall triangles).

`lsw_sim::world` collides with that data in this project's own way: find the highest walkable
triangle (normal y above 0.707, the game's own threshold) under the feet, follow steps up to
0.1 (estimated), push a circle out of wall lines and steep triangles, stop at ceilings.

**Scene objects and gizmos.** A named object's record (0xD0 bytes) points at its draw-list
record at +0xB0 and carries an object number at +0xC0. Collision groups of type 1 store that
number, which is how a piece of collision is tied to a scene object. Collision helper
objects are named `terr...` (`terr_cube` and similar) and are not drawn. Their visibility flag
byte is 0, but that byte is **not** a usable "hidden" test: it is also 0 on real scenery,
including the first room's walls.

The level's `.giz` file has a section per gizmo type. In `GizForce`, each entry starts with its
own name (`force1`, ...) followed by the names of its scene objects, each stored as a length
byte, the text and a zero. Only those names are read. In the first room `force1` is the exit:
56 bricks named `form`, `form1`..`form55` and the helper `terr_cube`, whose collision group
(object 856) is the slab that blocks the doorway. The sliding door `door_01` behind it is
stored already raised, and the wall lines leave a gap there.

Here: holding the Force button within 2 units for 1 second completes a Force object that has
a `terr...` helper; its bricks are moved aside by a made-up motion and the helper's collision
is switched off (`World::remove_object`). Range, time and motion are stand-ins. The level
script (`ai\level.scp`) shows the other doors are tied to path connections and object
animation frames; none of that is implemented.

**Camera rails.** Rails named `sock_cam_NN`, `sock_a_NN`, `sock_b_NN` share an index. The play
corridor lies between `a` and `b`; the camera rides `cam` at the same fraction along the
segment the player is in. From the level text, `cam_y_offset` with `offset_blend_y_ratio 1`
puts the camera that far above the player's feet, and `campos_seek` sets the position easing
(decomp `socksysall.cpp`). With `y_limits` that height is then clamped: never below the camera
rail itself, never above the `sock_mid_NN` rail at the same point (EnforceSockYLimits). In the
first room this holds the camera at about 0.7 whatever the player's height, so a jump tilts
the view up instead of lifting the camera through the ceiling. The target is the player's
middle. Not implemented: blending
between rails, look-ahead, lateral/vertical/pull-back ratios, distance-to-target, limits,
two-player framing.

## Model files as levels (`lsw/src/obj.rs`)

Not a LEGO Star Wars format: a plain `.obj` reader so any model can be a level. One draw batch
per material, textures loaded from the PNG files the `.mtl` names. Positions are scaled and
used as renderer coordinates as they are; game space is that with z negated, so nothing is
mirrored. Face direction is not trusted from the file: if the near-flat triangles mostly face
down, every triangle is flipped. Faces without texture coordinates are never drawn. When they amount to more than a twentieth
of the drawn triangles they are taken to be the stage's own collision shell and become the
collision; otherwise the drawn triangles are used. Drawing that shell was a bug: it showed as
flat grey slabs (every corner sampled one texel) and flickered against the real ground lying
in the same plane. PNG textures get a full chain of half-size copies so they do not sparkle
at a distance. Collision goes to
`lsw_sim::world` (floors where the normal's y is above 0.707, walls otherwise).

The chase camera for these levels is this project's own: 2.4 behind the character's heading
and 0.95 up, eased with the usual position seek, and shortened to 85% of the distance to the
first surface between the character and the camera (`World::segment_hit`).

## Lighting (`lsw_sim::lighting`)

From decomp `rtl.c` and the level's `.rtl` file (a raw table: 4-byte version, 128 light records
of 0x8C bytes, then fog records).

- Light kinds: 1 ambient, 2/3/6/8 point, 4 directional, 5 camera-relative directional,
  7 anti-light (dims others nearby).
- Strength at a position: 1 inside the inner radius, falling in a straight line to 0 at the
  outer radius.
- Per object the game keeps the 3 strongest ambient lights (summed, capped at 1) and the 3
  strongest others. A point light's direction is from the object towards the light. A
  camera-relative light is (0,0,1) turned by its own pitch and yaw and then by the camera,
  always applies, and has strength 1. Colour is the light's colour times strength times
  intensity.
- A vertex is ambient + sum of max(0, normal . direction) * colour, capped at 1, times texture.

In this project that sum is computed on the CPU for the character every frame (it is already
skinned on the CPU) and written as vertex colour; all materials are unlit and tone mapping is
off, so the picture is texture times light as in the original. The sum is done in display
space and converted for the renderer.

Level meshes: those without normals, or with descriptor bit 0x4000000 (an extra baked stream),
are baked scenery and use their vertex colours (0x7F = 1.0). The rest are free-standing LEGO
pieces whose vertex colours are neutral; they are lit once at load from the light table at
their own position.

Materials: the colour at +0x54 is a display-space value (the same bytes appear as 8-bit colour
elsewhere in the record), opacity is at +0x70, and the low 4 bits of the attribute word at
+0x40 are the alpha mode (0 opaque, otherwise blended). Vertex colour alpha uses 0x7F as fully
solid. Treating colours as linear, and drawing blended materials as solid, were both bugs
found by comparing with screenshots of the real game.

Reflective floors are not a render effect: the level file contains a mirrored copy of the room
below a see-through floor material. The character's reflection is a second copy of its meshes
flipped about the floor height, drawn only when the level text has `reflect_range`.

Glow is limited to pixels brighter than white (only the blade), so it cannot haze the scene.

Not done: specular, fog, light modifiers/animation and group masks beyond anti-lights, shadow
direction from lights. The flat test floor uses two made-up lights.

## HUD

The game does not draw its HUD from sprite sheets. Each piece is a small model (a textured
quad) in the shared props scene `stuff\things_ios.gsc`, found by name, and drawn in front of
the camera (decomp `panel.cpp`, `render.cpp` DrawPanel3DObject, DrawCharIcon, DrawHitPoints).

- A piece at panel position (x, y) with scale s is placed in camera space at
  (x * MULX, y * MULY, 1) and scaled by s, where MULY = f / ((1 - f) * 0.22 + 2.545),
  f = field of view / 0.75, and MULX = MULY * aspect ratio.
- Portrait ring `icon_back_blue` and the portrait (the object named after `icon=` in the
  character's icon scene) at (-0.775, 0.745), scale 0.16.
- Hearts `flat_heart` from x = -0.65 at y = 0.745 - 0.045, scale 0.195, stepping right by
  0.35 of the scale (times 0.85 on widescreen). The last full heart is drawn slightly nearer
  and pulses: scale times (1.2 - 0.2 * phase), phase running 0 to 1 twice a second.
- Stud `silver_pan_coin1` at (-0.65, 0.745 + 0.045), scale 0.35; the count starts at x = -0.61
  in colour (255, 191, 0).

The stud icon is animated by swapping its texture: the script `stuff\ats\coin.ats` steps from
the model's own texture through the next 31 in the scene's texture list and loops, one step
every second tick (taken here as 30 steps a second, which is an assumption).

Supporting formats found on the way:

- Scenes list their named objects in the display scene (count at +0x6C, 0xD0-byte records at
  +0x70, name at +0xB4, draw list at +0xB0), the same record a character piece uses.
- HUD textures are `sharedtextures\0x<hash>.pvrnc`: uncompressed PVR version 3 (here 16-bit
  RGBA 4444) so they stay sharp. 198 textures in the archive use it.
- The font (`.fnt`): glyph count at 0x1C, character count at 0x20, line height at 0x24, glyph
  boxes from 0x6C as (right edge x, top y, width) in texture pixels, then (character, glyph)
  pairs, then one DDS texture.
- Prop scenes store vertices at the stride their records state; only level scenes drop the
  tangent/binormal streams. The reader now accepts a measured stride only when it differs
  from the stated one by exactly those streams.

Here the pieces are drawn by a second, fixed camera on their own render layer so scenery can
never cover them. They are shaded with a made-up fixed light (they are flat quads, so it only
scales brightness).

## Combat (`lsw_sim::combat`, block rules in `lsw_sim::player`)

From the decomp:

| Rule | Source |
|---|---|
| Droid bolt ("red"): 4 units a second, 2 second life, 1 damage | `bolttypes_lsw.inc` |
| A bolt is "incoming" when closer than 0.5 s of travel and its line passes through the target | `bolt.cpp` FindIncomingBolt |
| With the saber out, on the ground, attack pressed while a bolt is incoming starts a block instead of a swing | `move.cpp` LightSabreComboCode |
| The block clip is picked at random from block1..3, never the same twice running | `pushblocks.cpp` NewBlockAction |
| A block winds up 0.3 s facing the bolt, then lasts while bolts come and the button is held; 0.2 s later it ends, into a held guard if the button is down | `move.cpp` BlockCode |
| A bolt reaching a player Jedi in the block state is sent straight back along its path | `bolts.cpp` Bolt_HitGameObjectRC |
| In the held guard it is turned away with up to 45 degrees of random yaw and a random pitch, and a fresh block clip plays | same |
| Otherwise it does its damage and adds the bolt's direction times the walk speed to the velocity | same |
| Battle droid: 1 hit point, clips `shoot`, `idle` and others | its character file |

Stand-ins (this project's numbers, because the game drives enemies from per-level scripts
that are not implemented): droids see 9 units, fire every 2.2 to 3.2 seconds, do not walk; the
saber reaches 0.75 in front; the muzzle is 0.28 up and 0.15 ahead of the droid; a droid has no
blaster model in its hand. Out of hearts, the player is simply refilled.

A destroyed droid is drawn as its own model pieces flying apart (each mesh of the model, thrown
up and outwards, gravity -5, resting on the floor, blinking out after 3 to 4 seconds). The
pieces are real; the speeds and timings are a stand-in for the game's debris system.

Not done: the player's hit and death clips, enemies taking more than one hit, enemy walking
and melee, droids in the LEGO level (their placement comes from level scripts), bolt sparks.

## Camera (`lsw_sim::camera`)

The game's camera has two halves (decomp `camera.cpp`). A per-level rail ("sock") system gives
a wanted position and target; the back half eases the camera position towards it
(`position_seek * dt`, default 5), computes yaw and pitch from camera to target, eases those
(`angle_seek`, default 5), and applies a decaying-sine judder.

The back half is implemented. In a level the front half uses the level's rails as described
under Levels. On the flat test floor (`--flat`) it is a stand-in: a fixed offset
(0, 0.98, -2.2) from the player's middle. The field of view (0.75 radians) is the engine
default, not a per-level value.

### Scenery between the camera and the player

Read from the decomp (`camera.cpp` `MoveGameCamera`, `socksysall.cpp` `SockSysCamera`,
`tagging.cpp` `Tag_DrawIcon_LSW`). The game does **not** collide its camera with scenery:

- Camera positions come from rails drawn by the level designers where nothing is in the way.
- A rail set may carry `cam_height_above_terrain`. If it is above 0 the game finds the ground
  under the camera (looking from the camera's height down 5 units) and, if the camera is lower
  than ground + that value, raises it to exactly that. Implemented in
  `World::lift_above_ground` and read from the level text file. The ground lookup itself
  (`NewShadowEx`) is not in the decomp, so which surfaces it counts is this project's choice
  (any upward-facing one).
- When scenery hides a player the camera stays put and the player is marked. Every other
  frame the game casts a ray from the camera to a random point in the upper half of the
  character's body; once the answer has been the same for 0.3 s it becomes the "hidden" flag.
  While set, the font's up-arrow character (0xAC) is drawn over the player in player one's
  colour (0, 127, 255), its opacity following 80 + 48 * sin over a half-second cycle (out of
  128). Implemented in `camera::HiddenCheck` and `place_hidden_marker`.
- `CamTiltDist`, `CamTiltHeight` and `TerrainCamInActive` are read from level files but
  nothing in the decomp uses them, so nothing was built from them.

Estimates and stand-ins in this part:

- The test area has no rails, so its fixed-offset camera borrows the height-above-terrain
  rule with this project's numbers: 0.5 clearance, ground looked for up to 3 units over the
  camera as well as below (the plain offset can land inside a ramp), and four extra points
  0.3 out so it rises before reaching an edge.
- The body region sampled for the hidden test uses the character's radius and half height;
  which two sizes the game uses there is not known.
- The arrow's size on screen (44 pixels) and its place at the top of the head. The game gives
  a text scale of 3 and a height field that were not traced.
- The font lists the arrow glyph as 12 wide but its picture is wider; it is drawn as wide as
  the down arrow (38), judged from how the glyphs are packed.

## Checking

- `cargo test -p lsw_sim`: speeds, tiers, jump height and time, double jump height, lunge,
  backflip, the full saber sequence, camera lag (speed / seek).
- `cargo run -p lsw -- --demo`: 16 screenshots in `logs/` and a numeric log.
- Not checked against the original game running side by side.
