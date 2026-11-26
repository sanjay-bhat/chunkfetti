---
name: chunkfetti
description: Set up and run chunkfetti end to end on this computer - check the system, get the builder, find the user's game install and saved worlds, build browser tiles from a world and open a local preview. Use whenever the user asks to install, set up, run, build, preview or update chunkfetti, or to "put my world in the browser", even if they don't name chunkfetti.
---

# chunkfetti: from saved world to browser, end to end

Work through these steps in order, telling the user in plain words what each one is doing. Stop and ask whenever a
step needs a download or touches their files outside this repo.

## 1. Check the system

Run `scripts/check.sh`. It reports the operating system and CPU, whether a `chunkfetti` binary is on the PATH or in
`./bin`, whether Node.js 20+ is installed (only needed for the preview), and Rust (only needed to build from source).

## 2. Get the builder

Prefer the released binary: from the repo's latest GitHub release, pick the file for this OS and CPU (`macos-arm64`,
`macos-x64`, `linux-x64` or `windows-x64`). **Before downloading, tell the user the file name, the release version,
the size shown on the release page and that it comes from this repo's GitHub releases, and wait for a yes.** Put it in
`./bin/`. On macOS, if the system blocks it, explain the quarantine flag and let the user decide.

If they would rather build from source and Rust is installed: `cargo build --release`. If Rust is missing, ask before
installing it (rustup.rs is the official installer).

## 3. Find the game and the world

Run `chunkfetti doctor`. It lists the game folders, installed game versions (their client jars hold the textures) and
saved worlds. Ask the user which world, and which game version to read textures from if there are several. For modded
worlds, ask where the mods folder is: modded blocks render from those jars.

Never copy game or mod files into this repository, and never commit them.

## 4. Build the tiles

```bash
chunkfetti build "<world folder>" -o "<world name>.tiles" [--mods "<mods folder>"] [--pack "<resource pack>"]
```

To try it quickly on a big world, build a small part first with `--area x0,z0,x1,z1` (block coordinates).

Defaults use a quarter of the CPU cores (at most 4), to keep laptops cool; only raise `--threads` if the user asks. Building a large world
takes a while: run it in the background and report when it ends. Rebuilding after the world changes only redoes the
tiles whose chunks changed.

## 5. Preview

With Node.js available: link the tiles folder into the viewer (`ln -s "<absolute path to tiles>" viewer/public/tiles`;
on Windows copy it), then `cd viewer && npm install && npm run dev` and open the printed local address. Drag to pan,
scroll to zoom, right-drag to tilt. Nothing is published anywhere by this step.

## 6. Publishing (only if asked)

The tiles and the built viewer (`viewer/dist`) are static files, so any static host works. Publishing makes renders of
their world public: confirm the user wants that and which host before uploading anything.

## Ground rules

- Every download needs the user's yes first, with name, version, source and size.
- No game or mod assets in this repo, ever.
- If something fails, show the error and the command that produced it; do not guess past it.
