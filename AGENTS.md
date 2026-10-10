# AGENTS.md

## Project
A Rust/Bevy rebuild of the core character engine of LEGO Star Wars: The Complete Saga (movement,
camera, animation, lightsaber attacks), reading the player's own copy of the Android game data
at run time.

## Hard rules, never break these

1. **Never write game assets, decompiled code, or extracted game files into this repository.**
   They stay on this machine, untracked. Read game data from the `.obb` at run time, or extract
   it to the gitignored `work/` folder for inspection.
2. **`.gitignore` is a whitelist.** It ignores everything and includes only this project's own
   source and notes. Do not switch it to a normal ignore list.
3. **Do not run `git commit` unless asked.** Stage nothing beyond what the current task needs.
4. **Do not touch anything outside this project folder** unless a path is explicitly named.
   The game archive is opened read-only; never write to it.
5. **Single-player and offline only.** No DRM, anti-cheat or online play work.
6. **Never put credentials in the repo.**
7. **The decompilation in `saga-main/` is reference material (GPL-3.0).** Read it to learn how
   the game behaves and record what was learned as plain-language notes in `docs/`. Write this
   project's code in its own structure; do not copy the decompilation's code, quirks or dead
   code. Do not run its scripts or binaries without asking.
8. **`ai-game-modding-guides/` is reference material too.** Do not run anything from it.

## How to work

- **Read the real thing, don't guess.** A struct layout, constant or rule comes from the real
  file, the format notes or the decompilation. If it was estimated, say so in the code and in
  `docs/DESIGN.md`.
- **Keep the design notes as you go** in `docs/DESIGN.md`.
- **One thing at a time.** Each change should be revertable on its own.
- **Log, don't look.** The app writes positions, states and events to `logs/lsw.log`, and
  `cargo run -p lsw -- --demo` plays a scripted run and saves screenshots to `logs/`.
  Rules live in `crates/lsw_sim` with numeric tests (`cargo test -p lsw_sim`).
- **Tell the user how to test it.** After every change, give the exact command and what they
  should see.
- **Ask before large refactors.**
- **Explain in plain language.**
- **No over-orchestration.** One agent, one chat, small steps.

## Honesty

- If something is not tested, write **"not tested"**.
- If unsure, say so.
- After two real attempts at one problem, stop and write `STATUS.md` instead of trying
  variations.
- Record failures alongside successes in `MODLOG.md`.

## Keep these files updated

- `MODLOG.md`: an entry after every change.
- `docs/DESIGN.md`: how the project works and where each rule came from.
- `README.md`: the "what works / what doesn't" list. Test before claiming something works.

## Environment

- OS: Windows 10 Pro
- Game: LEGO Star Wars: The Complete Saga, Android release, data file
  `main.1063.com.wb.lego.tcs.obb` (1,236,862,804 bytes) in the project folder
- Reference decompilation: `saga-main/` (opensagadev/saga, Android x86 build)
- Language: Rust stable (MSVC), Bevy 0.20
- Agent: Claude Code
