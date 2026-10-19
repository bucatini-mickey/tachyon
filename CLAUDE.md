# Tachyon: handoff notes

Mickey's game-agnostic engine for Dwarf-Fortress-style games that are real terminal games: pure ASCII on screen, real per-cell physics underneath, fast above all. Started 2026-10-07. Dated session notes are in `docs/HISTORY.md`.

## Goal
- One engine, several games: **Broadside** (naval, `games/broadside`; the Python original in `../Broadside` is inspiration only), a **gothic** Morrowind-like RPG, a **desert** Mad-Max-like wanderer, maybe a **spy** game. The engine knows about none of them.
- DF-style: a 3D grid of 1 m cells, shown one z-level at a time. A cell is drawn as its most important thing.
- Real per-cell physics: liquid in 64ths, heat, gas, light, fire; walls hide what is behind them.

## Decisions
- **Rust, crossterm the only dependency.** Mickey knows Python and is learning Rust: explain idioms when they come up. Barebones, easy to build, Linux-friendly; WASM/phones later.
- **Terminal-native output.** Games draw into `frame::Frame` with any glyphs and 24-bit colours; `display::Display` decides what reaches the screen: `Charset::Ascii` (default) folds everything to 7-bit ASCII, `Scheme` picks colours (`natural`, `terminal` = the terminal's own 16 colours and background, `phosphor`, `amber`, `paper`). Ctrl+T / Ctrl+G cycle them in any game; `TACHYON_SCHEME` / `TACHYON_CHARSET` set them. Only `backend/` touches crossterm.
- Backgrounds carry meaning only (classic roguelike): `MapView` draws floors and walls as coloured glyphs on black; liquid, fire, gas, blood, items on furniture, highlights and overlays colour the background. `MapView::fill = true` restores the old filled-tile look.
- The main loop draws only after input or a tick (plus a 100 ms idle refresh), so a paused game costs next to nothing.
- World storage is structure-of-arrays in 16x16x1 chunks; only awake chunks simulate (`world::Wake`), a slow sweep wakes sleepers.
- Materials (liquids and gases too) live in one registry; `Materials::standard()` is an optional starter set.

## Layout
Folders are layers; each uses only those above it. Every module is re-exported at the crate root (`tachyon::world`, `tachyon::craft`), so games never name the folders.
```
src/
  base/     color geom rng noise                 maths, wyrand RNG, value noise
  grid/     material tile world things clock     the cell world; items/furniture/creatures (slot map + spatial index)
            movement vessel scene                walking/stairs/swimming; rafts; Scene = everything ticked together
  sim/      fluid gas heat light flames          cellular automata over awake chunks (+ liquid pressure)
            weather projectile                   gusting wind and rain; things in flight, explode()
  life/     body needs mind ai path species      creatures: tissue-layer bodies, needs, routines, A*
  sea/      field craft ballistics sight         the sea as fields (tachyon::sea::Sea); ships posed on it; shot; long sight
  make/     atlas blueprint worldgen lang        authored world map + structures (text formats); generated names
  draw/     frame view/ ui display               cell buffer (+ to_html/to_text); map view; widgets; ASCII + colour schemes
  backend/  term input app                       diffed terminal writer; input events; Game trait + main loop
games/broadside/   the naval game (workspace member)
examples/          sandbox (test map), editor (atlas + 1 m builder), seaway (two brigs on a modelled sea)
tests/             one file per system; display.rs checks that what the engine draws reaches the screen as ASCII
```

## Who works where (parallel Claude sessions)
- "New engine vs. Broadside project": `games/broadside/`, `src/sea/`, `src/draw/ui.rs`, `src/make/lang.rs`, the pressure pass in `src/sim/fluid.rs`, `examples/seaway`.
- "Tachyon engine setup": `life/`, `examples/sandbox`, README.
- Message the owner before editing their files; append rather than rewrite.

## Checks
- `cargo test --release -p tachyon` (engine), `cargo test --release --workspace` (with Broadside).
- `cargo run --release --example sandbox -- --bench 600`: ~4.2 ms/tick on 160x100x12 (fluid ~1.9, heat ~2.2), draw ~1.6 ms.
- Snapshots: `--snapshot out.html` writes HTML + `.txt`; the sandbox applies `Display::from_env()`, so `TACHYON_SCHEME=terminal` shows that scheme. `snaps/` is gitignored; `../.claude/lum.js` reads a snapshot's brightness.
- Windows Terminal is the target. Running examples lock their .exe; close them before rebuilding.

## Working with Mickey
- Specific, efficient code that is easy to copy and paste.
- Be selective about markdown and docs; they cost credits.
- Main PC: Windows, "Rosemary". Rust via rustup (`~/.cargo/bin` may need adding to PATH in a new shell).
