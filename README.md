# Tachyon

A fast, game-agnostic engine for Dwarf-Fortress-style ASCII games in the terminal: a 3D grid of 1 m cells with liquids, gases, heat, fire and coloured light, viewed one level at a time. Written in Rust; its only dependency is `crossterm`.

## Install

1. Install Rust from <https://rustup.rs> (on Windows choose the default MSVC toolchain; it needs the Visual Studio C++ Build Tools).
2. Build and run the sandbox from this folder:

```
cargo run --release --example sandbox
```

Use Windows Terminal (or any true-colour terminal) with a font that has box-drawing characters, such as Cascadia Mono.

## Sandbox

| Keys | |
|---|---|
| arrows / `hjklyubn` / numpad | move; hold to walk, hold two arrows to go diagonally |
| `<` `>` | climb stairs |
| PgUp / PgDn | look at other levels |
| `x` | look mode; then `w` water, `r` lava, `f` fire, `s` smoke, `c` chill, `d` dig/build, `t` torch (place / light / take) |
| `f` | throw a firebolt: the game pauses, aim with the movement keys (`<` `>` change level), Enter throws, Esc cancels |
| `v` | see everything / only what you can see |
| `o` | overlays: temperature, liquid, gas, light, active chunks |
| `m` | two characters per cell (the default, square in most fonts) or one |
| `g` | lighting on/off |
| `U` | show hillsides above you (on by default) or plain walls |
| `L` / `C` | carry a lantern / close nearby doors |
| walk off the raft's edge, `J` | paddle the raft that way, jump overboard |
| `W` / `R` / `F` | change the wind / cycle rain / fog rolls in |
| `a` then a direction | attack (`K` changes weapon: fists, knife, club, spear, axe) |
| `E` / `D` / `Z` | eat food beside you / drink from water beside or below you / sleep (time runs fast) |
| space, `+` `-`, `T` | pause, speed, skip an hour |
| `?` / `q` | help / quit |

Headless options:

```
cargo run --release --example sandbox -- --bench 600 [--fire]
cargo run --release --example sandbox -- --watch 43200        (a game day: prints what the creatures did)
cargo run --release --example sandbox -- --snapshot out.html [--at x,y,z] [--hour 21] [--all] [--narrow] [--overlay temperature] [--fire-at x,y,z] [--bolt x,y,z] [--rain 0-2] [--wind 0-5] [--fog] [--ticks 300]
```

`--snapshot` writes an HTML page and a `.txt` of one frame (`--hurt` wounds you first).

The village lives on its own: villagers sleep in their beds, work, eat from the larder, drink at the water, and fight back or flee when attacked (and their friends, and the dog, come to help); the cat hunts the rats in the cellar; sheep graze. The panel shows your body (or, in look mode, the creature under the cursor): blood, pain, what you can still do, how you feel, and each wound. Bruises change colour as they heal; blood stains the floor until rain or water washes it away.

## World editor

```
cargo run --release --example editor               (edits world.atlas)
cargo run --release --example editor -- isle.atlas
```

Two scales. The **atlas** is the world map you author: each cell is a 32 x 32 m region with a height, a rock type and a biome, plus rivers drawn from their source and feature markers. Press Enter on any cell to step into the **1 m world** generated from it (3 x 3 regions). There you build with the mouse or keys, and save a selection as a **blueprint**. A saved or stamped blueprint is recorded on the atlas, so it is rebuilt there whenever that area is generated. The atlas and the blueprints (`blueprints/*.tbp`) are plain text.

| Atlas | |
|---|---|
| mouse / arrows | move the cursor; left paints, right does the alternative; wheel sizes the brush |
| `1`-`7` | raise/lower, set height, smooth, rock, biome, river, feature |
| `Tab` | view: terrain, height, rock, biome |
| `[` `]`, `+` `-` `PgUp` `PgDn`, `,` `.` | brush size; height / step / river width; choose rock, biome or feature kind |
| river tool | click points from the source; Enter finishes; right click undoes a point or deletes a river |
| `Enter` | walk into the 1 m world here |
| `u` / Ctrl+Z, `S` / Ctrl+S, `I`, `N`, `q` | undo, save, new rough island, blank sea, quit |

| 1 m world | |
|---|---|
| `1`-`6` | paint, fill rectangle, room (walls round a floor), furniture, water, select |
| `t`, `[` `]`, `,` `.` | tile shape, material, furniture kind |
| `<` `>` / wheel | level up / down |
| `S` / `B` | save the selection as a blueprint / stamp one at the cursor |
| `P`, `u`, `Esc` | run the physics (rivers flow), undo, back to the atlas |

`--snapshot out.html [--view n] [--local x,y] [--ticks n] [--keys "..."]` draws one screen headlessly; `--keys` plays a key script (e.g. `"Enter 3 Space Right*7 Down*5 Space"`).

## Tests

```
cargo test --release
```

`tests/physics.rs` checks the invariants: liquid is conserved, falls and levels; heat is conserved, including across sleeping chunks; field of view is symmetric; wood burns to ash and fire spreads; water on lava makes obsidian and steam. `tests/vessel.rs` checks rafts (floating, carrying passengers, stopping at banks, drifting downstream, conserving water) and the direction pad. `tests/worldbuild.rs` checks atlas and blueprint text round trips, stamping, and that generation is correct and deterministic. `tests/life.rs` checks bodies (bruises age and heal, bones break and lame, blades bleed and sever, heart wounds kill, small creatures reach only legs), blood stains, routes, finding food and water, routines, fighting back and fleeing, allies, hunting, drowning, burning and falling. `tests/elements.rs` checks flames in water, wind and rain, fog drifting, wind-driven fire, blast shielding, and firebolts.

## Architecture

```
color geom rng noise       utilities
material tile world        the grid, structure-of-arrays, in 16x16x1 chunks that sleep when nothing changes
things                     items, furniture, creatures (a `deck` kind can be stood on anywhere)
vessel                     rafts and boats: float on the surface, carried by currents and wind, push a wake
clock scene movement       time of day; Scene = world + things + clock + sim + weather + vessels + projectiles; walking, stairs, doors, swimming
weather projectile         gusting, veering wind and rain; things in flight and explosions
atlas blueprint worldgen   the authored world map; stampable structures; atlas -> 1 m world
life/                      creatures: bodies of tissue layers and wounds, needs, minds with routines and grudges, pathfinding
sim/                       fluid (+ flow field, springs and drains), gas (drifts with the wind), heat + fire, flames on things, light (cellular automata over awake chunks)
view/                      renders a level into a Frame: looks down through open space and up at hillsides; lighting, field of view, memory, overlays
frame input term app       cell buffer; the crossterm backend and main loop (the only terminal code)
```

A game implements `app::Game` (tick, draw into a `Frame`, handle keys). Because only `term` and `app` touch the terminal, another backend (web canvas, native window) only has to display a `Frame` and send key events.
