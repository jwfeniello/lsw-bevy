# MODLOG

## 2026-10-09 - first playable build

**Changed**
- `tt_formats`: archive reader with TT's deflate variant, `.ghg` model, `.atitc` texture,
  `.an3` animation and character-file readers; `ttdump` inspector.
- `lsw_sim`: Jedi character controller, clip player, follow camera, 9 numeric tests.
- `lsw`: Bevy 0.20 app with keyboard/mouse/gamepad input, CPU skinning, saber, sounds, HUD
  text, log file and a scripted `--demo` mode.
- Project files: whitelist `.gitignore`, `AGENTS.md`, `README.md`, `docs/DESIGN.md`.
  `git init` run; nothing committed.

**Tested**
- `cargo test -p lsw_sim`: 9 passed.
- `ttdump ... check chars\quigonjinn quigonjinn_lr_ios.ghg idle 0`: 66 animations parsed, 9
  textures decoded, posed feet at y = 0.
- `cargo run -p lsw -- --demo`: exit 0, 16 screenshots looked at, log numbers match the rules
  (jump peak about 0.45, 0.78 s in the air).

**Not tested**
- Gamepad. Interactive play by a person. Any character other than Qui-Gon. Side-by-side
  comparison with the original game.

**Dead ends and fixes along the way**
- First model parse read past the end: layer handles are 12-byte records, not direct pointers,
  and draw-list indices are 32-bit. Found by dumping the real file.
- A slam test failed because it pressed attack too late: the game only allows it while falling
  slower than 1.25. The test was wrong, not the rule.
- Bevy 0.20 renames: `Hdr` is in `bevy::camera`, `DirectionalLight::shadow_maps_enabled`,
  `FontSize::Px`, `GlobalAmbientLight`. `Image::new` only accepts the top mip level; the full
  chain is assigned afterwards.
- First framing was too far away and washed out; camera offset and lighting retuned by eye.

**Game files**
- The `.obb` is only ever opened for reading. Inspection copies of Qui-Gon's folder are in
  `work/lego/` (gitignored); delete that folder to remove them.

## 2026-10-09 - traced movement rules, first real level

**Changed**
- Rules: the game's clip selection, clip hold timers, jump-to-fall switch, 0.2 s grace jump,
  heavier landing after a long fall, 1-based event frames. Confirmed the saber-away trigger.
- `tt_formats`: descriptor-based vertex layouts, level scene meshes and splines (`ghg.rs`),
  collision file reader with wall lines (`ter.rs`), `ttdump ... ter PATH`.
- `lsw_sim::world`: collision against level triangles and wall lines, camera rail lookup.
- `lsw`: loads and draws `negotiations_a` by default (`--flat` for the old test floor,
  `--level=` for another folder), blob shadow.
- `.ignore` added so search tools can still see `saga-main/` now that git ignores it.

**Tested**
- `cargo test -p lsw_sim`: 9 passed.
- `cargo run -p lsw -- --demo` in the level: exit 0; screenshots show the meeting room,
  character lit, blob shadow, saber; the log shows no falling through the floor and walls
  stopping the run.
- `cargo run -p lsw -- --demo --flat`: exit 0, 21 events logged (same sequence as before).

**Not tested**
- Any other level. A person playing. Gamepad.

**Dead ends and fixes**
- Level vertex data looked out of range: the files are 4 or 8 bytes per vertex smaller than
  their headers say (tangent/binormal not stored).
- Fell through the floor a short walk from the start: I had picked the wrong second triangle
  for each collision quad. The points are a strip (0,1,2 then 1,2,3).
- Walked through the room's side wall: rooms are fenced by wall lines in the collision file,
  not by wall triangles.
- The character was black: the ceiling shadowed the test light. Level meshes no longer cast
  shadows.
- `git init` made the search tool skip `saga-main/` (it honours `.gitignore`), which silently
  returned "no matches" for a while.

## 2026-10-09 - the game's lighting and shading

**Changed**
- `tt_formats::rtl`: level light-table reader; `ttdump ... rtl PATH` lists the lights.
- `lsw_sim::lighting`: the game's per-object light selection and per-vertex shading.
- `lsw`: character and saber hilt shaded by those rules every frame; free-standing level
  pieces shaded once at load; baked scenery unchanged; renderer lights and tone mapping
  removed; all materials unlit.

**Tested**
- `cargo test -p lsw_sim`: 9 passed. Demo in the level and with `--flat`: exit 0. Screenshots
  looked at: door and chairs now show their studs and form, the character takes the room's
  cool ambient plus two camera-relative key lights.

**Not tested**
- No side-by-side comparison with the original game's picture.

## 2026-10-09 - corrections from real-game screenshots

The project owner supplied two screenshots of the real game in the first room.

**Changed**
- Glow now only comes from the blade (it was adding a haze over the whole picture).
- Material colours treated as display-space values (LEGO greys were washed out to white).
- Blended materials drawn see-through with their opacity and vertex alpha (they were solid
  black). This revealed the mirrored room under the floor: the game's floor reflection.
- Character and saber hilt mirrored under reflective floors. Black backdrop in levels.

**Tested**
- Level demo: exit 0; screenshots compared by eye with the references.

**Still different from the references**
- Table too dark, character slightly dim, no blade reflection, no studs/sparkles.

**Note**
- The game window was open during a build, which locks the program file. The in-use file
  was renamed to `target/debug/lsw_in_use.exe` rather than closing the window; it can be
  deleted once that window is closed.

## 2026-10-09 - table and blade, from two more screenshots

**Changed**
- Materials now read their environment-map slot. Glossy level materials add the map's average
  colour at half strength (a stand-in for real reflections; the strength is an estimate).
- Saber blade: pale core plus a wide additive glow in the blade colour; neither is shaded by
  the room lights.

**Tested**
- Level demo: exit 0. Screenshots: table is dark purple-brown instead of black; blade is a
  thick green glow with a pale core.

**Not done**
- Real environment-map reflections, a rounder glow shape, blade reflection in the floor.

## 2026-10-10 - HUD from the game's own pieces

**Changed**
- First attempt drew the ring, hearts and stud by hand. The project owner rejected that, and
  rightly: the decomp shows the HUD is made of named models in `stuff\things_ios.gsc`.
- `tt_formats`: named objects of prop scenes (`Model::specials`), uncompressed PVR textures
  (`.pvrnc`), the game font (`fnt.rs`); unreadable meshes in prop scenes are skipped and
  counted instead of failing the whole file.
- `lsw`: HUD built from `icon_back_blue`, the character's portrait object, `flat_heart` and
  `silver_pan_coin1` at the game's panel positions; stud count in the game font; debug text
  moved to the bottom-left with F1 to hide it.

**Tested**
- Level demo: exit 0; screenshot shows ring, portrait, stud, "0" and four hearts top-left.
  Portrait centre is within a few pixels of its place in a widescreen screenshot of the game.
- `cargo test -p lsw_sim`: 9 passed.

**Dead ends and fixes**
- Stud count showed "1" for "0": glyph boxes store the right edge, not the left.
- HUD models were invisible: the camera-follow system was also moving the HUD camera.
- HUD models were white squares: their textures are `.pvrnc`, a format not handled before.
- Prop scene failed to load: the level-file stride workaround mis-fired on it by coincidence.

**Not right yet**
- Pieces are about 1.3 times smaller than in the reference screenshot; cause unknown.
- `things_ios.gsc` still reports 198 skipped meshes (not ones the HUD uses); not investigated.

## 2026-10-10 - spinning stud, HUD size settled

- The project owner supplied a Complete Saga screenshot and the stud sprite sheet.
- HUD size and position agree with the Complete Saga screenshot; the earlier "1.3 times too
  small" came from comparing against the 2005 game.
- The stud icon showed only frame 0 (edge-on). It now steps through the 32 consecutive
  textures the game's own script uses. Tested: demo exit 0, 32 frames loaded, a mid-spin
  frame visible in a screenshot. The step rate (30 a second) is assumed.

## 2026-10-10 - camera height limits

- Reported by the project owner: jumping lifted the camera out of the map.
- Cause: the rails' `y_limits` setting was not implemented. The game clamps camera height
  between the camera rail and the `sock_mid` rail.
- Tested: demo log shows camera height 0.720 standing, mid-jump and after a slam (it was
  1.0 rising to 1.45 before). Screenshot of a double jump: camera inside the room, tilted up.
- Side effect, expected from the data: the camera now sits lower than before all the time.

## 2026-10-10 - leaving the first room

- Asked: make the doors open so the room can be left.
- Found by reading the files: the exit is not blocked by a sliding door (`door_01` is stored
  raised) nor by wall lines (they leave a gap). It is blocked by the collision of an invisible
  helper object, `terr_cube`, which the gizmo file lists in Force object `force1` together
  with the 56 bricks of the big LEGO door.
- Changed: level meshes and collision carry their scene object name/number; invisible helpers
  are no longer drawn; `GizForce` entry names are read; holding the Force button near such an
  object completes it, moves its bricks aside and removes the helper's collision.
- Tested: demo log shows `ForceComplete(force1)` and the character running from x = -4.6 to
  x = -0.9 through the doorway; screenshots show the open door and the corridor beyond.
  `cargo test -p lsw_sim`: 9 passed.
- Stand-ins: Force range and hold time, and the bricks' motion. Not done: every other door,
  lift and Force object.
- Dead ends: assumed collision groups referred to draw-list indices (they use the object
  number at +0xC0); the Force entry's own name has no length byte, so the first reader found
  no entries.

## 2026-10-10 - walls restored

- Reported by the project owner: the previous change removed the walls of the room.
- Cause: it skipped drawing every placed object whose visibility byte was 0, on the evidence
  of one object (`terr_cube`). That byte is also 0 on walls and other scenery.
- Fix: only objects named `terr...` are skipped, and blockers are picked by that name too.
- Tested: demo exit 0; screenshots show the room's walls back, the door still opening and the
  character passing through; log still records `ForceComplete(force1)`.
- Lesson: one example is not enough to give a flag a meaning.

## 2026-10-10 - plain model files as levels

- Asked: use a City Escape stage model (an `.obj` with 124 PNG textures, supplied as a zip)
  as the level. The zip was unpacked to `work\city_escape\` (ignored by git).
- Added `crates/lsw/src/obj.rs`, `--obj=` and `--obj-scale=`, drawing and collision from the
  model, a start point, and a chase camera with a line-of-sight check.
- Tested: demo with the model exits 0; 131,216 triangles and 124 materials load; screenshots
  show the character on the road with trees, sky and the hill below; a frame that was blank
  (camera behind a building wall) is fixed by the line-of-sight check. `cargo test -p lsw_sim`
  passes.
- Not tested: playing through the stage. Scale (0.05) is chosen by eye.
- Launch note: the path has spaces, so the whole `--obj=...` argument must be quoted.

## 2026-10-10 - model level: grey sides and flashing ground

- Reported by the project owner: the sides were completely grey and textures flashed on the
  ground.
- Cause: 14,245 faces in the file have no texture coordinates. They form an untextured copy
  of the whole stage (534 objects spanning the full level): its collision shell. Drawn, they
  were flat grey and fought with the visible ground in the same plane.
- Fix: such faces are not drawn and are used as the collision instead (15,427 triangles after
  splitting); textures get mip levels.
- Tested: demo in the model level exits 0; screenshots show buildings, ivy walls, trees and
  road with no grey slabs; the character still stands and runs on the road.
- Not tested: whether any shimmer remains while moving (screenshots are still frames).

## 2026-10-10 - faster in model levels, free flight

- Asked: make him faster in the model level only, and add a key to toggle flying.
- Model levels multiply tiptoe/walk/run speeds by `--obj-speed=` (default 5). The LEGO level
  and the flat floor keep the real numbers. Acceleration, turning and jumps are unchanged.
- F (gamepad top face button) toggles free flight in any mode: camera-relative movement at
  12 units a second (40 with Shift), Space up, Ctrl/C down, no gravity or collision; the
  character rules are not stepped while flying. Turning it off drops him under normal gravity.
- Tested by the demo in the model level: run speed reaches about 5.7 units a second; a
  two-second flight climbs from height 0 to 14.4 and travels about 18 units; afterwards he
  falls at gravity -6. The F key itself and the gamepad button were not pressed by a person:
  **not tested**.

## 2026-10-10 - built-in test area, back to normal speed

- Asked: keep the model-level work, go back to a test area with ramps and such, normal speed.
- The model-level option is unchanged (`--obj=`, faster there only). Nothing was committed.
- New default mode: a test area built in code (`crates/lsw/src/testarea.rs`). `--lego` now
  selects the game's first level; `--flat` the bare floor.
- `--demo` in the test area prints an automatic walk-and-jump check.
- The check found two faults in this project's collision code, both fixed:
  - risers lower than the step limit acted as walls, so 0.10 steps could not be climbed;
  - the step-up allowance applied in mid-air, so jumps landed on platforms 0.1 above their peak.
- Also a fault in the check itself: it kept the stick pushed after landing and walked off the
  far side, reporting height 0. It now records the peak and releases the stick.
- Tested: the check's final output (see README); `cargo test -p lsw_sim` 9 passed; demo exit 0.
- Not tested: the wall, the low slab, and anything by hand.

## 2026-10-10 - battle droids, bolts, blocking; test-area camera

- Asked: is the camera back to how it was; add droids that shoot, can be destroyed, and whose
  bolts can be blocked back at them, using the decomp for behaviour.
- Camera: the test area had been left on the chase camera written for model files. It is back
  on the fixed-angle follow camera; the chase camera is for `--obj` levels only.
- `lsw_sim::combat`: bolts with the game's speed, life and damage; the game's incoming test;
  reflect-back on a timed block and scatter on a held guard; damage and knock-back otherwise.
- `lsw_sim::player`: block state with the game's timings and random block clip, held guard
  playing block clips on a deflection, hit points.
- `lsw`: the battle droid loaded from the game data; five placed in the test area; bolt
  visuals; pieces fly apart on destruction; hearts on the HUD follow hit points; sounds for
  shot, hit, deflect and droid destroyed.
- Tested: `cargo test -p lsw_sim` 12 passed, including an unblocked hit (4 hearts to 3 with
  knock-back), a timed block that returns the bolt and destroys the shooter, and a saber
  swing that destroys only the droid in front and in reach. Demo exit 0 with 28 shots, 12
  hits (hearts counting 4, 3, 2, 1 and refilling), 1 deflection and 1 droid destroyed logged;
  screenshots show droids, bolts in flight and two hearts left.
- Not tested: blocking by hand (timing by a person), the held guard in the running game, the
  falling-apart effect by eye (no screenshot happened to catch it).
- Open question: each droid has a round red part on its body. It is drawn with the colour the
  model file gives; whether the plain battle droid shows it red in the game was not checked.
- Stand-ins are listed in `docs/DESIGN.md` under Combat.

## 2026-10-10 - Camera and scenery (test area camera went inside ramps)

- Asked: the camera goes inside ramps when the character walks behind them; find out how the
  game handles it.
- Found in the decomp: the game does not collide the camera at all. Rails are authored clear
  of scenery; a rail set can ask for a minimum camera height above the ground under it
  (`cam_height_above_terrain`); a player hidden by scenery gets an arrow drawn over them
  after 0.3 s of the camera-to-body ray being blocked. Three other rail settings
  (`CamTiltDist`, `CamTiltHeight`, `TerrainCamInActive`) are parsed but unused in the decomp.
- `lsw_sim`: `World::lift_above_ground`, `Sock::height_above_terrain`,
  `GameCamera::stand_in_goal`, `camera::HiddenCheck`. `lsw`: `cam_height_above_terrain` read
  from level files, the test-area camera uses the lifted goal, `place_hidden_marker`,
  `--start=x,z`, and a camera line in the test-area report.
- Tested: `cargo test -p lsw_sim` 14 passed (2 new: camera rises to box top + 0.5 and nothing
  is above it; hidden flag sets after about 0.3 s and clears). Demo report: of 9345 standing
  spots the plain offset put the camera inside scenery at 295, with the lift 0. Screenshot
  from `--demo --start=-10.25,18.2`: camera above the ramp's landing, arrow over the hidden
  character.
- Failed first: the arrow drew as a tiny sliver, because the font lists that glyph as 12 wide
  while its picture is about 38 wide. An attempted fix did nothing because a PowerShell patch
  with a single pair silently flattened; redone as a normal edit.
- Not tested: by hand in the window; the `--lego` level (no rail set there was checked for
  `cam_height_above_terrain`, and the arrow there is untried); the arrow's pulse by eye.
- Estimates are listed in `docs/DESIGN.md` under Camera.

## 2026-10-10 - README brought up to date on enemies

- The README still said "No enemies" and "hearts never empty", gave 9 tests, and called the
  test-area camera a chase camera. Corrected: enemies and bolts are now in the summary and
  under "What works", with their limits under "What is approximate or missing".
- Notes only; no code changed.
