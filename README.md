# LEGO Star Wars character engine on Bevy

A from-scratch Rust/Bevy rebuild of the core character engine of *LEGO Star Wars: The Complete
Saga*: Qui-Gon Jinn walking, jumping and fighting with his lightsaber inside the first level
(the Negotiations meeting room), followed along the level's own camera rails. It reads the models, textures, animations, sounds and character settings from **your own
copy** of the Android game data at run time. No game files are in this repository.

Unofficial fan project, not affiliated with or endorsed by the game's owners. Experimental.

## Run it

You need Rust (rustup.rs, MSVC toolchain on Windows) and the Android data file
`main.1063.com.wb.lego.tcs.obb` (1,236,862,804 bytes) from a copy of the game you own, placed in
this folder (or pass its path as the first argument).

```
cargo run -p lsw
```

| Control | Keyboard / mouse | Gamepad |
|---|---|---|
| Move | WASD or arrows (Shift = walk, Ctrl = tiptoe) | Left stick |
| Jump (again in the air = double jump) | Space | A / Cross |
| Attack / draw saber | J, Enter or left mouse | X / Square |
| Put saber away, or use the Force (hold near a Force object) | K, E or right mouse | B / Circle |
| Fly on/off (while flying: Space up, Ctrl or C down, Shift fast) | F | Y / Triangle |

Moves: attack on the ground chains up to three swings (press again as each swing ends); attack
during a single jump is a lunge; attack after a double jump is a slam; throw the stick the
opposite way and jump straight away for a backflip; jump just after a swing for the combo jump.

Other commands:

```
cargo run -p lsw               # the built-in test area (ramps, stairs, platforms, gaps)
cargo run -p lsw -- --lego     # the game's first level (the Negotiations meeting room)
cargo run -p lsw -- --flat     # a bare flat floor
cargo run -p lsw -- --level=levels\episode_i\negotiations\negotiations_b   # another level folder (not tested)
cargo run -p lsw -- --demo     # scripted run: saves screenshots to logs/ and exits
cargo test -p lsw_sim          # numeric tests of the movement rules
cargo run --release -p tt_formats --bin ttdump -- main.1063.com.wb.lego.tcs.obb list chars\quigonjinn
```

`logs/lsw.log` records position, velocity, state, clip and camera four times a second, plus
every event.

## The test area

With no option the game starts in a test area built in code (this project's own, nothing from
the game), at the character's real speeds, with a chase camera:

- **Ahead:** six ramps at 10, 20, 30, 40, 50 and 60 degrees, each 1.5 high with a landing.
- **Behind:** three staircases with steps 0.05, 0.10 and 0.15 high.
- **Right:** platforms 0.2, 0.4, 0.5, 0.7, 0.8 and 1.0 high.
- **Left:** a raised walkway with gaps 0.5, 1.0, 1.5, 2.0 and 3.0 wide.
- **Further behind:** a wall, and a slab 0.6 above the ground.
- **Five battle droids** around the start that turn to face you and shoot.

Fighting them:

- A hit costs a heart and nudges you back. At no hearts you are restored at once.
- **Timed block:** with the saber out, press attack just as a bolt is about to hit. The bolt
  goes straight back at the droid that fired it.
- **Held guard:** keep attack held after a swing or a block and bolts are knocked away at a
  random angle instead.
- A saber swing destroys a droid in reach; a returned bolt destroys the one it hits. A
  destroyed droid falls apart into its own pieces.

The test area uses the LEGO-style camera that keeps a fixed angle and follows; the chase
camera is only for model-file levels. The game never pushes its camera around scenery, so
neither does this: the camera is kept a set height above whatever ground is under it (so it
rides up over a ramp instead of going inside), and when the ramp then hides the character a
blue arrow marks where they are, as in the game. `--start=x,z` starts somewhere else, for
example `cargo run -p lsw -- --start=-10.25,18.2` starts behind the first ramp.

`cargo run -p lsw -- --demo` prints a check of it before the window opens. Result on
2026-10-10: ramps up to 40 degrees are climbed and 50 and 60 are not (the game treats ground
steeper than 45 degrees as wall); 0.05 and 0.10 steps are climbed and 0.15 are not (the 0.10
step limit is this project's estimate); a single jump (peak 0.44) lands on platforms up to
0.4 and a double jump (peak 0.75) up to 0.7; a running single jump clears the 0.5 gap and a
double jump the 1.0 gap. The wall and the low slab were **not** part of the automatic check.

## Using another model as the level

Any Wavefront `.obj` file on your disk (with its `.mtl` and PNG textures beside it) can stand
in as the level. Faces with texture coordinates are drawn. If the file also carries an
untextured copy of the stage (stage rips include the game's collision shell this way), that
copy is used for collision and not drawn; otherwise the drawn triangles are collided with.

```
cargo run -p lsw -- "--obj=work\city_escape\city escape\city escape.obj"
cargo run -p lsw -- "--obj=<file.obj>" --obj-scale=0.05
```

- In a model level only, he moves 5 times faster than his LEGO speeds (run 6 units a second
  instead of 1.2). Change it with `--obj-speed=`; `--obj-speed=1` gives the real speeds.
- `--obj-scale` is how many game units one model unit is (default 0.05; a minifig is 0.42
  game units tall). It is a guess per model, not something read from the file.
- The start point is the ground above the model's origin, or the highest flat triangle.
- There are no camera rails in such a model, so a simple chase camera is used (this project's
  own). It pulls in front of walls that come between it and the character.
- The model is shown at full brightness; the character uses the test lights. No reflections,
  doors, or anything else from the LEGO level data.
- Tested with one model (a 131,000-triangle city stage): loads, the character stands and
  runs on the road, the demo runs to the end. Walking the whole stage was **not tested**.
  Jump heights are unchanged, so gaps built for a faster, higher-jumping character may not
  be crossable on foot; free flight (F) gets past them.
- Model files stay on your machine: keep them under `work\` (ignored by git). A model ripped
  from another game is that game's copyrighted art; do not publish it or commit it.

## What works (tested on Windows 10, Bevy 0.20, 2026-10-09)

Checked by the scripted demo (screenshots + log) and by `cargo test -p lsw_sim` (9 tests):

- **The first level, `negotiations_a`**: its scenery (2,154 placed meshes with their textures
  and baked vertex-colour lighting), its collision (5,090 triangles plus 358 wall-line
  segments), its start point and its 15 camera rails with their height setting.
- Walking on the level floor, being stopped by walls and furniture, low steps.
- The game's blob shadow under the character (darkness from the level file; size estimated).
- **The game's lighting model**: the level's light table (60 lights in this level), with each
  object lit by its three strongest ambient and three strongest directional/point lights,
  shaded per vertex; scenery uses its baked vertex-colour lighting; no extra tone mapping.
- **The reflective floor**: the level file holds an upside-down copy of the room under a
  tinted see-through floor, and that is drawn as stored; the character is mirrored under it
  the same way. See-through overlays (soft shadows, glass) blend as their materials say.
- **The HUD, from the game's own pieces**: the blue portrait ring, the character portrait, the
  hearts and the stud icon are the game's models and textures (from the shared props scene and
  the character's icon scene), placed with the game's own panel positions and sizes; the last
  heart beats as in the game; the stud count uses the game's font. F1 hides the debug text.
- Compared by eye against two screenshots of the real game in this room (supplied by the
  project owner): overall brightness, grey LEGO colours, floor reflection and framing are
  close; see the gaps below.

- Reading the `.obb` archive directly, including its non-standard compression.
- Qui-Gon's model (rigid and skinned pieces, layers 0/2/5), textures, and all 66 of his
  animation files; the lightsaber hilt and blade attached to his hand.
- Movement with his real numbers: tiptoe 0.154 / walk 0.6 / run 1.2 units per second, the
  stick dead zone and speed tiers, acceleration, turning, camera-relative control.
- Jump (2.3 up, gravity -6: peak 0.44, 0.77 s), quick re-jump and double jump (peak 0.75),
  backflip, lunge, slam with camera kick, landing clips.
- Saber draw / put away with the blade extending on the clip's frames, the two alternating
  three-swing combo chains with timed "b" branches, hit frames, combo jump.
- Clip blending with each clip's own blend time and frame rate.
- Camera easing, look-at and landing judder; the camera kept above the ground under it and
  the arrow over a character hidden by scenery (checked in the test area only; the 0.5
  clearance there, the arrow's size and its exact place are estimates).
- Saber on / off / hum / swing sounds.

## What is approximate or missing

- **The camera uses the level's rails in a simplified way.** It rides the rail for the corridor
  the player is in, at the rail's configured height held within the rail's height limits
  (so it stays inside the room when you jump), looking at the player. The game's rail
  system has dozens more settings (look-ahead, lateral and pull-back ratios, blending between
  rails, two-player framing) that are not implemented, so framing can differ from the game.
- **Collision uses the game's shapes but this project's method** (probe down for a floor, push
  out of walls). The game's own terrain routines, step height, slope sliding, moving
  platforms, surface types and water are not reproduced.
- **Leaving the first room works the game's way, partly.** The exit is a LEGO door that is a
  Force object in the level's gizmo file, with an invisible collision block behind it. Hold
  the Force button near it for a second and the bricks move aside and the block's collision
  is removed, so you can walk into the corridor. What is real: which bricks belong to the
  door, the blocker, and that the Force is what opens it. What is a stand-in: the distance
  (2 units) and hold time (1 second), and the bricks' motion (they drift sideways and up; the
  game's stored motion is not decoded, and the moved bricks can hang in the camera's view).
- **Everything else collidable is fixed.** Sliding doors, lifts and the other Force objects do
  not work yet, so the corridor's side doors stay shut. No pickups, switches, build-its,
  enemies or level scripts.
- **Every placed mesh is drawn**, including a few that the game hides or swaps (a purple marker
  cube is visible in a doorway; lower-detail copies may overlap).
- **Seen in the comparison and only partly fixed**: glossy materials (the meeting table) carry
  an environment map that is not drawn; as a stand-in its average colour is added at half
  strength, so the table has roughly the right tone but no moving reflections. The blade is
  drawn as a pale core inside a wide coloured glow like the game's, but its shape comes from
  the thin rod model widened 3.2 times (an estimate) and looks boxy up close. The character
  is a little dimmer than in the game; the blade is not reflected in the floor; studs,
  sparkles and other effects are absent.
- **Lighting gaps**: no specular highlights (the plastic shine), no normal maps, no fog, no
  light flicker/animation or light groups, and the fixed LEGO pieces in the level take their
  camera-relative lights from one frozen view angle. Shading is per vertex; whether the game
  shades per pixel on this platform was not checked. The 0x7F = full brightness scale for
  baked colours is read from the data, not confirmed against the game's shader.
- Only `negotiations_a` has been loaded. Other levels are **not tested**.
- **No enemies**, so blocking, the swipe-behind move, targeting and damage are not built. Hit
  frames are detected and logged but hit nothing.
- Idle variations, head turning and cloak/hair physics are missing.
- Three movement details are still unconfirmed: the lunge's rise (the file's `lunge_jumpspeed`
  is not handled by the reference code), whether steering is cut during the fall clip, and the
  exact order of steps within a frame.
- **HUD gaps**: size and position agree with a Complete Saga screenshot to within what a
  small image can show (the 2005 game's HUD is larger; that is a version difference). The
  stud icon spins through the game's 32 frames, at an assumed 30 frames a second. The stud
  count's size on screen is an estimate, it always reads 0 (there are no studs to collect),
  hearts never empty (nothing hurts you), the heart spacing may be slightly wide, and the
  layout assumes a 16:9 window. No pause menu, hints,
  second-player side or True Jedi bar.
- Not built: normal maps, face animation (`.bsa`), blade motion trail, footstep and jump
  sounds, Force powers.
- Gamepad input compiles but is **not tested**. Only Qui-Gon has been loaded.
- The blade glow is tuned by eye, not matched to the game.

## Layout

- `crates/tt_formats` - readers for the archive, models (`.ghg`), textures (`.atitc`),
  animations (`.an3`) and character files, plus the `ttdump` inspector.
- `crates/lsw_sim` - the rules (character controller, clips, camera) with no renderer, and
  their tests.
- `crates/lsw` - the Bevy app.
- `docs/DESIGN.md` - how it works and where each rule came from.
- `lego-star-wars-formats.md` - the file format notes.

## Credits

- File format notes: `lego-star-wars-formats.md`.
- Behaviour research: the [opensagadev/saga](https://github.com/opensagadev/saga)
  decompilation (GPL-3.0) was read as a reference for how the game behaves. No code from it is
  included; this project's code is its own structure.
- Project method: [AI Game Modding Guides](https://github.com/trevaintdead/ai-game-modding-guides).
- Built with [Bevy](https://bevyengine.org). Written with an AI coding agent (Claude Code).
