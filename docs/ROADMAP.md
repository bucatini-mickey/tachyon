# Tachyon and Tokamak: audit against the vision, and the roadmap (tach-013)

Written 10 Nov 2026 on `queue/tachyon` (main merged in at that date). Proposal only: no engine code changed.
Read for it: `CLAUDE.md`, `README.md`, `docs/HISTORY.md`, `docs/INTERACTIONS.md`, `docs/TOKAMAK_BACKLOG.md`,
`docs/design/SKILLS_AND_TIME.md`, `docs/design/NPC_MINDS.md`, and the code in `src/{grid,sim,life,make,sea,draw}` and
`examples/tokamak`. Measured: `cargo test --release` (130 tests, all pass) and `examples/sandbox --bench 600`.

Sizes: **S** one short run (under ~150 lines changed), **M** one full run, **L** two or more runs or a design step first.
Every item names the test that would prove it done.

## 1. The short version

1. **The bones are good.** Structure-of-arrays cells, sleeping chunks, conserving liquid and heat, tissue-layer bodies, a
   real sea and real shot. 130 tests pass. A quiet 160x100x12 map costs 4.2 ms a tick (fluid 1.9, heat 2.1), a burning one
   7.6 ms. Keep all of it, and everything Broadside, Torpor and Grimoire use (section 9).
2. **Five things in the present design block the vision** (section 3): the world is one fixed array per scene (no streaming,
   no world-scale fields); physics rates are per tick while life and the planned skills are per second and per day; wind
   and rain are one number for the whole map; a cell holds one liquid and one gas; and nothing lives above the 1 m grid
   except the atlas, so rivers by erosion, climate, belief fields and far NPCs have nowhere to run.
3. **The fix for most of it is one new layer, not a rewrite:** a **region layer** at atlas scale (32 m cells, 2D) that
   holds slow fields (wind, climate, hydrology, influence, reputation, far people), with the 1 m `World` as a local
   bubble generated from it. Section 3.6.
4. **Sound has no module.** A design is in section 5: an event, not a field; straight-line loss through walls plus a short
   search round corners; a noise floor from rain, wind, water and fire; heard levels feed the NPC stimulus bus.
5. **Order:** time foundations first (cheap, unblocks skills and NPCs), then sound (self-contained, wanted by Torpor),
   then the region layer with rivers by erosion. Tokamak's next steps wait on your try in tach-012.
6. **Three bundles are proposed** in `Queue/bundles.md`: time foundations, sound and hearing, rivers by erosion.

## 2. What was here before this audit

- **The 10 Nov "Tachyon engine ASCII terminal audit" chat** left nothing new in the repo on 10 Nov. Its work landed on
  main between 22 Oct and 4 Nov: the 16-ink palette as the default scheme (`src/draw/palette.rs`, 4 shades per ink),
  filled-tile drawing removed, and the web pseudo-terminal `web/pterm.ts` (row-diffed flush, fit-by-font, bold, wide
  glyphs, combining marks). Grimoire keeps its own copy of pterm. Those commits are now merged into `queue/tachyon`.
- **Queue runs since 31 Oct:** INTERACTIONS table (tach-001, tach-002: 20 new interaction tests), Tokamak backlog
  (tach-004), the two design proposals (tach-006 skills and time, tach-007 NPC minds), figure-lab (tach-008), and
  Tokamak's file formats moved into `src/make/library.rs` with safe names, line-numbered errors and atomic saves
  (tach-009 to tach-011). This roadmap builds on those; it does not repeat them.
- **Stale docs:** `CLAUDE.md` still says the default scheme is "natural" (it is `Palette`), its layout omits
  `library.rs`, `palette.rs` and `web/`. Fix in passing (S, no test).

## 3. Where the present design blocks the vision

### 3.1 The world is one fixed array per scene
`World` (`src/grid/world.rs`) is flat `Vec`s of `w*h*d`; chunks are only a wake mask. Per cell: 25 bytes of state plus
~19 bytes of scratch held elsewhere (`Sim::delta` 2, `heat_scratch` 12, pressure marks 4, `MapView::seen` 1): about
44 bytes. The sandbox (0.19 M cells) is 8 MB. One square kilometre at 60 levels is 60 M cells, 2.6 GB. Whole-world passes
also grow with it: `light::compute_sky` (every cell, whenever terrain changes, throttled to every 4th tick: 0.3 ms in the
fire bench), `Wake::collect` and `finish`, `rain()` (random columns over the whole map), `flames::step` and
`Life::step` (every thing and creature every tick, asleep or not).
**Blocks:** large worlds, travel ("a long walk should feel long"), anything persistent across places.
**Not a problem for:** Broadside (each ship is its own small scene on the field-based `Sea`).

### 3.2 Physics runs per tick; life runs per second
Liquid moves a level per tick, heat conducts per tick, `Material::burn_ticks`, gas decay, `flames::SOAKED` (600 ticks),
weather gusts (periods of 7, 19, 110 ticks) and wind (cells per tick) are all tick-based. `Life::step`, wounds, needs and
stains use game seconds. Games choose `secs_per_tick` (sandbox 2 s, Broadside 0.5 s), so the same fire burns four times
faster in game time on a ship. tach-006 wants all time as a fraction of a day with day length a setting.
**Blocks:** a day-length setting that means anything for the world, fast-forward (sleep, long jobs, travel) that keeps
fire and water believable, and comparing numbers between games.
**Fix:** a stated contract (section 8, T2): CA rates stay per tick and the engine defines one physics tick as 1 game
second; time-like rates that are not CA (drying, gusts, weather, burn duration of things) move to seconds; a game with
a different tick runs physics in whole sub-steps or accepts the scale; `Scene::fast_forward` runs ticks without drawing
and stops on events.

### 3.3 One wind and one rain for the whole map
`World::wind` and `World::rain` are scalars set by `Weather::apply`. Gas drift, fire spread, flames and vessels read
them wherever they are. There is no lee behind a hill or a house wall (only "open sky or not"), no valley wind, no storm
that crosses the map, rain falls straight down (`sim::rain`, though `weather.rs` says it slants).
**Blocks:** "wind moves smoke, fog, rain and flames" in a believable way, storms, rain shadow, climate.
**Fix:** a wind and rain field per chunk column (16x16 m, 2D; cheap: 60 values for the sandbox) with shelter from the
terrain upwind, fed by the region layer. Every reader of `world.wind` takes a position.

### 3.4 One liquid and one gas per cell
`fluid/fluid_mat` and `gas/gas_mat` hold one material each. Two liquids "simply do not mix" (`fluid::react`); a
different gas is overwritten when a larger puff arrives (`World::add_gas_idx`), which also destroys gas.
**Blocks:** oil on water (and burning oil), steam condensing into a puddle beside smoke, gas bubbling up through
water, firedamp in a smoky mine.
**Fix, cheap and enough at 1 m:** keep one liquid per cell but let lighter liquids float (density on `MatKind::Liquid`,
swap cells vertically when the lower is lighter); keep one gas per cell but stop destroying gas on displacement (push
the loser to a neighbour). A second gas channel only if a game needs it.

### 3.5 Liquids have no momentum
The CA levels surfaces; currents are smoothed flux (`flow_x/flow_y`). A river flows because springs and drains hold a
slope, a wave cannot travel, water does not overshoot a weir. Good for caves, rooms, holds, canals.
**Blocks:** rivers that behave like rivers over kilometres, floods with a front, erosion at runtime.
**Fix:** not in the 1 m grid. Rivers and lakes get their shape and discharge from the region layer (3.6, section 4.9);
the 1 m CA keeps them full with springs whose rates come from discharge. A shallow-water (virtual pipes) solver on a
2D column grid is the later step if floods are wanted (L, after the region layer).

### 3.6 Nothing between the atlas and the 1 m grid
The atlas (`src/make/atlas.rs`) is static authored data; `worldgen::generate` turns 3x3 regions into a `World` and then
nothing at that scale runs. NPC_MINDS needs district influence fields (64x64 cells), a far tier of bodiless people, a
place graph; SKILLS_AND_TIME needs off-screen catch-up; erosion, climate and weather need a 2D grid over the whole map.
None of these fit in `World`: it is local, 3D and chunked 16x16x1 per level, so a 2D field would be duplicated per
level, lost when the bubble moves, and sized by the bubble rather than the world.
**Fix: `src/region/` (new layer between `make/` and `grid/`)**: a `Region` grid the size of the atlas, 2D, one record
per atlas cell (32 m): elevation and relief, discharge and flow direction, soil moisture, temperature and wind, plus
open slots for the game's fields (influence per god, reputation, population). Ticked slowly (once per game hour or day).
The `World` becomes the **bubble**: generated from the region on entry, its changes saved as a per-chunk diff on exit
and reapplied when the area is generated again. That keeps every CA unchanged.

## 4. System by system

Each block: **Exists** (file, function), **Sound** (conservation, stability, cost), **Missing**, **Next** in order.

### 4.1 Heat and thermodynamics
- **Exists:** `sim/heat.rs` `step` (6-neighbour explicit diffusion, convection factor 1.6 up / 0.4 down, sky drift to
  `World::ambient` at 1%/tick, deep-earth drift, `edge` hand-over into sleeping chunks), `phase` (melt, freeze, boil),
  `burn`, `burn_out`; `Material::{conduct, heat_cap, melt, freeze, boil, self_temp}`; `Clock::ambient_temp`.
- **Sound:** conserved (tested, including across sleeping chunks); stable while `conduct <= ~0.12` (documented, not
  enforced). 2.1 ms/tick, the biggest single cost. Not physical: no latent heat (ice melts the tick it passes 0.5 C; a
  cell of water freezes whole, a full cell into a wall, the moment it dips below 0; boiling clamps the temperature and
  throws the heat away); fire adds 25 C/tick whatever the heat capacity; outdoor air is one number from the hour.
- **Missing:** latent heat, frost and snow, temperature by altitude, season and biome, creatures losing or gaining heat
  from cells (they read air temperature only), heat from bodies, stability check on registered materials.
- **Next:** H1 assert or clamp `conduct` at registration (S; test: a material with conduct 0.5 is clamped and a hot
  spot never overshoots its neighbours). H2 latent heat as a per-cell accumulator used only in cells at a phase point
  (M; test: a 1 m ice block at 20 C air takes a believable time to melt and total energy is conserved within 1%).
  H3 outdoor temperature from the region layer: lapse rate 6.5 C/km, season, biome (M, after R1; test: a hilltop
  60 m up is 0.4 C colder, January colder than July). H4 tile-level frost and snow cover (S after H3 and W1).

### 4.2 Fluids and hydrodynamics
- **Exists:** `sim/fluid.rs` `step` (gravity, spread by a fifth of the difference from a snapshot, trickle with
  `seek_low`, evaporation of thin films), `pressure` (each brim-full body as a pipe, budget 10k cells, body cap 4096),
  `react` (water on lava), flow field `record`; `sim::Source` springs and drains; `grid/vessel.rs` rafts.
- **Sound:** conserved exactly except evaporation (by design; tested). Stable. 1.9 ms/tick on the sandbox, mostly the
  flowing canal. Limits: no momentum (3.5); a body over 4096 cells is only partly levelled per tick; viscosity is a skip
  count, so lava moves in jumps; liquid does not soak into soil or pass grates.
- **Missing:** immiscible liquids (3.4), porous ground and a water table, doors and grates passing liquid, flow
  through thin gaps, a pressure test on a large lake.
- **Next:** F1 density layering for liquids (S-M; test: oil poured on water ends on top, both conserved). F2 porous
  floors: soil and sand absorb a little and release it slowly to a per-column moisture value (M; test: a puddle on soil
  shrinks, one on stone does not, water is accounted for). F3 springs whose rates come from region discharge (S after
  R2). F4 a shallow-water solver for open water on the column grid (L; later; test: a dam break front advances at about
  sqrt(g h)).

### 4.3 Gas
- **Exists:** `sim/gas.rs` `step` (decay, faster under open sky; rise; wind drift under open sky; conserving diffusion);
  materials steam, smoke, mist.
- **Sound:** diffusion conserves except at the 255 clamp; displacement by `add_gas_idx` destroys the smaller gas. Cheap
  (0.09 ms). Gas blocks sight above `opacity` but does not dim light.
- **Missing:** gas on creatures (smoke inhalation, poison), condensation, flammable gas, gas through liquid, fog made
  by the weather.
- **Next:** G1 stop destroying displaced gas (S; test: puffing steam into smoke keeps the total of both within 1%).
  G2 gas dims dynamic light by density (S; test: a torch behind a smoke bank lights the far wall less). G3 smoke and
  steam in the lungs: a `breath` drain by gas material (S-M, `life/needs.rs`; test: a creature in thick smoke for a
  minute collapses, in thin smoke coughs and lives). G4 condensation of steam on cold surfaces (S after H2).

### 4.4 Fire
- **Exists:** `sim/heat.rs` `burn` (ignite above `ignite`, `spread` chance, wind bias downwind, rain and water quench,
  smoke), `World::ignite`, `World::quench`; `sim/flames.rs` for carried flames (torches, lanterns, drying, gusts,
  `try_light`).
- **Sound:** every rule in the vision holds and is tested (no fire underwater, wet torch hard to light, water and rain
  put it out, runs downwind, no rain indoors). Burning cells are the main cost driver (fire bench 7.6 ms/tick, of which
  about 3.4 ms is outside the timed systems, so not measured: likely `Life` pathing away from fire; add a `life_us` stat).
- **Missing:** furniture, items and clothes burning (fire lives only in tiles), embers carried downwind (spotting),
  fuel moisture (green vs dry wood, wet thatch), burning creatures spreading fire.
- **Next:** X1 a `life_us` and `things_us` timing in `SimStats` (S; test: the bench prints them). X2 flammable things:
  `ThingKind::burns` (ignite temperature, burn time, leaves), lit by fire or heat in their cell, turned to their remains
  (M; test: a table beside a burning wall catches, burns, leaves ash, and lights the floor if flammable). X3 fuel
  moisture per cell derived from rain and liquid (S after F2). X4 embers: a burning exposed cell under wind has a chance to
  light a flammable cell 2-5 cells downwind (S; test: in a gale, fire jumps a 2-cell gap downwind, never upwind).

### 4.5 Light
- **Exists:** `sim/light.rs` `compute_sky` (whole world, on terrain change), `LightMap::compute` (coloured flood over
  the view plus 18 cells, linear falloff 14/step), `gather_sources`; `Clock::daylight`; `draw/view` lighting and
  `view/fov.rs` FOV.
- **Sound:** correct, tested (doorway leak, glass, dynamic light, submerged flames). Costs: `compute_sky` is O(world),
  `LightMap` is per frame and per view. Creatures do not perceive light: `ai::sees` ignores darkness.
- **Missing:** light for gameplay away from the camera (stealth, plants), water and smoke dimming, moonlight.
- **Next:** L1 `compute_sky` per dirty chunk column instead of the whole world (M; test: the same `sky` field as the
  whole-world pass after random edits, and the fire bench's sky time falls). L2 `light_level(p)` for AI: sky times
  daylight plus a cheap sum of nearby `ThingKind::light` sources (S; test: a villager in an unlit cellar is not seen by a
  guard 6 cells away; with a lantern he is). L3 moonlight from a calendar (S after T1).

### 4.6 Sound
- **Exists:** nothing in the engine. Torpor fakes it (`town.rs` `hearers`: walls halve the reach), Grimoire has whisper
  rules. The NPC_MINDS stimulus bus (section 6) assumes it.
- **Next:** section 5 (bundle `b-tachyon-sound`).

### 4.7 Weather and wind
- **Exists:** `sim/weather.rs` `Weather::apply` (prevailing speed and direction, gusts from value noise, veer, rain
  0..1) writing the scalars `World::wind`, `World::rain`; `sim::rain` drops; mist as a gas.
- **Sound:** deterministic, cheap. One value for the map (3.3), gust periods in ticks (3.2), rain straight down.
- **Missing:** wind field and shelter, rain slant, storms that move, snow, hail, lightning, fog formation, seasons,
  rain that feeds rivers.
- **Next:** W1 wind and rain field per chunk column with shelter by upwind terrain (M; test: the lee of a 3-level wall
  has under half the open wind for about 5 cells; smoke drifts round a hill). W2 rain slants with the field (S; test:
  rain lands under an overhang on the downwind side only). W3 moving weather cells on the region layer (M after R1;
  test: a storm crosses the map in hours and rain in a bubble starts and stops with it). W4 snow and lightning (M after
  H3; lightning strikes the highest exposed cell, lights trees, never indoors).

### 4.8 World generation
- **Exists:** `make/atlas.rs` (`Atlas`, elevation i16 per 32 m cell, rock, biome, rivers as polylines, features),
  `make/worldgen.rs` `generate` (interpolated height plus noise, jittered borders, soil and rock columns, steep slopes
  bare, ramps, carved monotone river beds, sea held by edge springs and drains, trees, puddles), `examples/editor`.
- **Sound:** deterministic and tested; regions line up. Generated area is limited by memory (3.1). River beds descend
  monotonically but water is one level deep regardless of the river; no lakes (basins just fill or drain).
- **Missing:** rivers from the land itself, lakes, geology (strata, caves, ore), climate, soil from rock and climate,
  the stability check "generated terrain does not flood or collapse" (INTERACTIONS).
- **Next:** see 4.9 for rivers; then G-1 settle test: generate the sample island, run 600 ticks, liquid outside
  channels and sea under 1% (S; can be written now). G-2 strata and caves: dipping rock layers from the atlas rock,
  karst caves in limestone below the water table (M after R2). G-3 streaming the bubble with saved diffs (L, R3).

### 4.9 River generation by erosion
- **Exists:** authored river polylines; carving in `worldgen`. No flow routing, no erosion.
- **What it needs from the layout:** a 2D heightfield over the whole map with flow direction, drainage area and
  discharge per cell (the region layer), and a finer relief than 32 m for valleys to have a shape. It does **not** need
  the 1 m grid: erosion happens at world-making time on the 2D field; the 1 m world only carves the result.
- **Plan (bundle `b-tachyon-rivers`):**
  R-a flow routing on the atlas: priority-flood depression filling (lakes recorded, not erased), D8 directions, drainage
  area times rainfall gives discharge; rivers extracted above a threshold, width ~ k sqrt(Q); authored rivers kept as
  pinned channels (M; test: on a cone island every river ends at the sea or a lake, flows downhill, and the largest
  river has the largest catchment).
  R-b stream-power erosion (dh/dt = U - K A^m S^n, implicit O(n) solver of Braun and Willett 2013) plus thermal slope
  limit, K from rock (soft chalk and sandstone erode faster than granite and basalt), on a relief layer upsampled from
  the atlas (8 m); writes a new atlas file, never the input (L; test: deterministic, soft rock loses more height, valley
  cells sit lower than their banks, total removed matches total deposited plus what left at the coast within 1%).
  R-c worldgen from discharge: river width and depth, lakes as flat-filled basins with an outflow, spring and drain rates
  from discharge (M; test: the G-1 settle test passes on an eroded island).

### 4.10 Projectiles and ballistics
- **Exists:** two systems. `sim/projectile.rs` (land: straight line, no gravity or drag, cells per tick, bursts with a
  `Blast` via `explode`, `shielding`, `trace`) and `sea/ballistics.rs` (real: gravity, sphere drag through Mach 1, exact
  quadratic-drag step, DDA through posed grids, wall resistance, ricochet off water, per-cell damage).
- **Sound:** both tested. The land one cannot throw an arrow, a stone or a bullet: no arc, no drop, no wind, and a hit on
  a creature is only a blast (`Life::blast`), never a pierce.
- **Next:** P1 one flight model: land projectiles use `ballistics::Shot` integration in cell space (1 cell = 1 m) with
  the wind field, keeping `Blast` as an optional payload and `trace` for aiming (M; test: an arrow at 45 degrees lands
  near its computed range; a crosswind of 10 m/s moves it sideways a measurable amount; Broadside's ballistics tests
  unchanged). P2 strikes on creatures: point and edge damage by mass and speed through `Body::strike`, with the part chosen
  by height of the hit (S after P1; test: an arrow at chest height can reach the heart, a sling stone bruises).
  P3 fire arrows and rain on them (S).

### 4.11 Creature bodies
- **Exists:** `life/body.rs` (`BodyPlan` trees, tissue layers, organs, `strike`, wounds, bleeding, clotting, healing,
  bruise colours, `capacities`, `fatal`), `life/needs.rs`, `life/species.rs` (human, dog, cat, rat, sheep), stains.
- **Sound:** the strongest part of `life/`; tested (14 life tests). Cheap per creature. Heals per day with `86400`
  hard-coded (T1).
- **Missing:** armour and clothing as outer layers, carrying and weight, tending wounds, infection, heat and cold
  damage (frostbite, heatstroke as tissue harm), smoke in the lungs (G3), more plans (bird, fish, serpent), ageing.
- **Next:** B1 worn layers: a `Worn` list per creature of extra tissue-like layers per part (padding, mail, plate, wool)
  that `strike` passes through first (M; Torpor row 33; test: mail turns a cut into a bruise, does nothing against a
  heavy blunt blow). B2 carried weight from things in an inventory, slowing `moving` (S after T3). B3 bandage and
  tourniquet as jobs (S after T5).

### 4.12 NPC minds
- **Exists:** `life/mind.rs` (`Mind`, `Routine` of `Duty`, `Goal`, grudges, `faction: u16`), `life/ai.rs` (`act`,
  `urgent` every 6 ticks, `plan` every 60, `look_around` over all creatures, `find_food` over all things), `life/path.rs`
  (A*, budget 4000 cells). The design for the rest is `docs/design/NPC_MINDS.md` (layered mind, People registry, facts,
  stimuli, places, aims, creeds, line bank). It stands; this roadmap only places it.
- **Sound:** behaviour is tested; cost is O(N^2) in `look_around` and O(things) in `find_food`; `Life::step` runs every
  creature every tick whether its chunk is awake or not.
- **What the layout must give it:** a creature index per chunk (both scans), a stimulus bucket per chunk (16x16 m is a
  good hearing cell), and for the slow social fields (belief, influence per god, reputation) a 2D map keyed by atlas
  region that outlives the bubble: the region layer (3.6). Influence fields are numbers per region record, ticked once a
  game day; "the effect of NPC thoughts on the world" is those numbers scaling existing rates (temper thresholds,
  plant growth when it exists, reporting of crimes), as NPC_MINDS 13.2 says.
- **Memory of places:** see section 6.
- **Next (from NPC_MINDS section 19, with the engine pieces here):** N1 temperament per creature (S). N2 chunk index for
  creatures and things (S-M; test: same behaviour, bench 500 creatures). N3 stimulus bus fed by sound (after S1-S3).
  N4 places and access (M). N5 People, persona, ties (M). N6 routines with `where_at` and wake-time placement (M, after
  T1). N7 facts and gossip (M). N8 line bank and matcher (M, can run in parallel after N5). N9 portal graph (M-L).
  N10 aims, creeds and influence fields on the region layer (M each, after R1).

### 4.13 Skills and the time tasks take
- **Exists:** nothing in code beyond `Capacities` and `Life` working in seconds. `86400` is hard-coded in `clock.rs`,
  `needs.rs`, `body.rs` (twice) and `life/mod.rs`. Walking speed is `Species::walk` in ticks per step. Design:
  `docs/design/SKILLS_AND_TIME.md` (Span, one duration formula, Job, skills table).
- **Next (bundle `b-tachyon-time` takes T1 to T3):** T1 `Clock::day_secs` and `Span` (S). T2 the physics time contract
  of 3.2 with `Scene::fast_forward` (M). T3 species speed in metres per second, several cells per tick allowed (S-M).
  T4 `life/skills.rs` (S). T5 `duration()` and `Job` (M). T6 eat, drink and sleep as jobs (M). T7 journeys and estimates
  (M). T8 off-screen catch-up (M-L, after R3).

### 4.14 Tokamak
- **Exists:** `examples/tokamak/main.rs` (Part, Library and Art tabs, undo, pickers, mouse, mirror, rect and room, flood
  fill, sockets per cell with a direction and tag) and `src/make/library.rs` (`Entry`, `Part`, `Socket`, `Art`, parsers
  with line numbers, `write_atomic`, `clean_file_name`). The atlas editor (heights, rocks, biomes, rivers, features) is a
  separate program, `examples/editor`. `tokamak/broadside` is untracked and has a library but no parts or art yet.
- **Missing for the vision:** a World tab (the atlas editor folded in); roads; snapping parts on sockets; any assembly
  of parts; loading a plan and turning it into a generation schema; a link from library materials to physics
  `Material`s; NPC routines placed in the editor. Usability gaps are in `docs/TOKAMAK_BACKLOG.md` (items 3 to 7 and
  12 to 20 still open).
- **Design and next:** section 7.

## 5. Sound: design for `src/sim/sound.rs`

**Principles.** Sound is an **event**, not a field: nothing runs per tick when it is quiet. Levels are decibels (dB SPL
at 1 m from the source). One function answers "what does this listener hear of that sound". Deterministic. ASCII output
is the game's business: the engine says what was heard, how loud above the background, and from which way.

**Types (plain Rust).**
- `Sound { pos: Pos, db: f32, kind: SoundKind, source: Option<ThingId> }`. `SoundKind` is a small enum the game can
  extend by id: Speech(loudness), Scream, Footstep, Impact, Blast, Collapse, Fire, Water, Animal, Bell, Custom(u16).
- `Heard { sound: usize, db_over: f32, from: (i32, i32, i32) }` where `from` is the direction it seems to come from
  (the first opening on the path, so a shout round a corner seems to come from the door).
- `Material::sound_loss: f32`: dB lost through a 1 m wall of it. Defaults: air 0, leaves 3, wood 22, glass 25, soil
  40, stone and rock 50, ice 30. Closed door (`ThingKind::door`) 18; floor slab 35 (wood) or 45 (stone).
- `Scene::sounds: Vec<Sound>` filled during the tick; `Scene::hear(listener_pos, hearing_db) -> Vec<Heard>`.

**Loss along a path.** `loss = 20 log10(max(r, 1)) + 0.005 r + walls`, r in metres. Walls: sum `sound_loss` of wall
cells on the straight line (the same sampling as `projectile::shielding`), capped at 60. Heard if
`db - loss >= floor + hearing`, with `hearing` per species (dog better than human).
**Round corners.** When the straight line crosses a wall, also run a small Dijkstra from the source through open cells
and open doors (cost 1 dB per cell plus 6 dB per turn through an opening, budget 2048 cells, radius 32 m) and take the
better of the two. Indoors this is what carries a scream down a corridor; outdoors the straight line is enough.
**Noise floor.** One number per chunk column per tick, from things already simulated: rain on open sky (+0 to 20 dB),
wind speed (+0 to 25 dB), flowing water from `flow_x/flow_y` (+0 to 30 dB beside a weir), burning cells (+10 to 30 dB).
Quiet night indoors about 25 dB; a storm outside 60. So rain hides footsteps and a whisper is lost by a waterfall.
**Typical levels.** Whisper 30, speech 60, shout 80, scream 90, footstep 20 to 45 by mass and floor (sand quiet, wood
loud), sword on shield 95, collapse 100, musket 140, cannon 160, blast `100 + 20 log10(power)`.
**Cost.** A shout outdoors to 30 listeners: 30 line walks of under 100 cells, a few microseconds each. The round-corner
search runs only when the line is blocked and only inside its budget. Footsteps are emitted only by creatures within the
player's hearing range or a chunk with a listening AI.
**Wiring.** Life events (struck, scream on a wound, fell, died), blasts, collapses (`burn_out`, broken walls), fire
starting, doors opening, and footsteps emit sounds. Heard sounds become NPC stimuli (NPC_MINDS section 6): sleeping
creatures wake above a threshold, AI turns to or investigates. The player sees a log line ("You hear a scream to the
north-east.") and, if the game wants, a brief glyph at the edge of sight toward `from`.
**Tests:** a shout through one wooden wall is heard at 10 m, through four stone walls is not; closing a door between two
rooms drops the heard level by at least 15 dB; a scream in a stone room with an open door is heard outside along the
corridor and seems to come from the door; rain raises the floor enough that a whisper at 3 m is lost outdoors but heard
indoors; a sleeping dog wakes at a footstep that does not wake a sleeping human; the sandbox bench costs under 0.1 ms
more with 20 walking villagers.
**Out of scope:** echoes, pitch, music, sound through water (later, for the sea).

## 6. Memory of places (player and NPCs)

**Exists:** `MapView::seen: Vec<bool>` (whole world, per view, never fades); remembered cells are drawn grey with
furniture only.
**Vision:** a village explored once and revisited next day shows its layout but not most interiors; a week later even
that has faded unless you knew it well.
**Design.** A per-observer `Memory` keyed by chunk (sparse `HashMap<chunk, ChunkMemory>`), so it costs nothing where you
have not been and can be saved with the game:
- per cell a `u8` "last seen" in coarse time (quarter days, wrapping is fine with a cap) and the cell's class at that time
  (open ground, road, outer wall, inner wall, furniture, landmark);
- per chunk `familiarity` (time spent there, and visits, saturating).
Recall strength = `familiarity_factor x exp(-age / half_life(class))`, with long half-lives for roads, outer walls and
landmarks (weeks), short for interiors and furniture (a day). Drawn at three strengths: full remembered colours (greyed),
outline only (roads, outer walls, landmarks), nothing.
NPCs use the place graph for the same purpose (they know places, not cells); their memory of where a person was is a fact
(NPC_MINDS 9.2).
**Next:** M1 `Memory` in `draw/view` replacing `seen` (M; test: a room seen once is fully remembered after 1 hour, outline
only after 2 days, gone after 10; a road walked daily is still drawn after 10 days; memory of an unvisited area allocates
nothing).

## 7. Tokamak: roads, snapping parts, plans to schemas

**T-a Fold the atlas editor in (M).** A fourth tab, World, from `examples/editor/atlas_mode.rs` (heights, rock, biome,
rivers, features). Keep `examples/editor` until the tab is at parity. Test: the editor's key-script snapshots reproduce
in the tab.

**T-b Roads (M).** On the atlas: a road is a polyline like a river, with a width, a surface material and a class (track,
road, paved). `worldgen` grades it: the bed follows the terrain smoothed along the path, cut and filled to a slope limit,
with ramps; it crosses rivers with a ford or a bridge part (a socket-tagged part from the library). In a part: a road
brush that lays the surface material and marks `road` sockets on the part's edge. Test: a road across a hill never has a
step steeper than one level per 3 cells; it meets a river with a bridge when the river is deeper than 1 level.

**T-c Snap parts on sockets (M).** Rules: two sockets join when their directions oppose (n to s, up to down) and their tags
match (or a tag table says they may). `library::assemble(a, sa, b, sb) -> Placement` gives B's offset; a preview shows two
parts joined; a check lists unmatched sockets and overlaps. Rotation and mirroring of parts (with sockets turned) come
with this. Test: a wall part with `wall` sockets e and w joins a tower part's `wall` socket w with no overlap; a `door`
tag refuses a `wall` tag; a rotated part's sockets point the right way.

**T-d Castle components (S-M, mostly content).** A starter kit in the Broadside or Torpor project: wall run, corner tower,
gatehouse, keep, stair tower, each with sockets on a 4 m module. Test: the kit tiles a square curtain wall of any size
that is a multiple of the module.

**T-e Plans to generation schemas (L, design step first).** Input: a plan as text or a `.tart` picture where glyph
classes mean roles (`#` wall, `O` tower, `G` gate, `K` keep, `.` court), or later an image. Steps: (1) classify cells by
role; (2) find components (connected runs of one role) and their joins; (3) emit a **schema**: a graph of slots, each
with a role, a size range and the sockets it needs, plus constraints (symmetry, gate faces the road, keep in the inner
ward); (4) a generator fills each slot with a library part that has the role and fits the sockets, choosing by seed, and
places them with T-c. Store schemas as text (`.tschema`) beside parts. Test: the plan of a square castle gives a schema
of 4 walls, 4 towers, 1 gate; three seeds give three different castles that all keep the plan's footprint and
connectivity. Needs your call on the input format (Decide item).

**Also:** library materials linked to physics materials (a `.tlib` material names a `Materials::standard()` entry or
gives `conduct`, `ignite`, `strength`, `sound_loss`) so a part stamped into a game burns and blocks sound correctly
(S-M). NPC routines and places as atlas features once N4 and N6 exist.

## 8. The order of work

Dependencies run downward. Phase 1 items are independent of each other.

**Phase 1: foundations (small, unblock the rest)**
- T1 day length and Span (S) - T2 physics time contract and fast-forward (M) - T3 speed in m/s (S-M)
- X1 life and things timing (S) - G-1 worldgen settle test (S) - H1 clamp conductance (S) - G1 keep displaced gas (S)
- CLAUDE.md refresh (S)

**Phase 2: sound, perception, cheap realism**
- S1 sound core (M) - S2 round corners (M) - S3 sounds from the world and hearing (M)
- N1 temperament per creature (S) - N2 chunk index (S-M) - L2 light level for AI (S)
- X2 flammable things (M) - X4 embers (S) - G3 smoke in the lungs (S-M) - B1 worn layers (M)
- M1 memory of places (M)

**Phase 3: the region layer and the land**
- R1 `src/region/`: the 2D record per atlas cell, ticked hourly; climate fields (M-L)
- R-a flow routing (M) - R-b stream-power erosion (L) - R-c worldgen from discharge (M)
- W1 wind field (M) - W2 rain slant (S) - W3 moving weather (M) - H3 temperature by place and season (M)
- F1 liquid density (S-M) - F2 porous ground (M)

**Phase 4: people and places**
- T4 to T7 skills and jobs - N3 to N9 of NPC_MINDS - P1, P2 one flight model and strikes
- Tokamak T-a to T-d (after your tach-012 verdict)

**Phase 5: scale**
- R3 the bubble: per-chunk diffs, re-centring, saving (L) - T8 off-screen catch-up - N10 aims, creeds and influence
  fields - L1 sky per chunk column - T-e plans to schemas - F4 shallow water

## 9. Keep as it is (other projects depend on it)

- **Broadside** (`../Broadside`, path dependency): `sea::{Sea, SeaSettings, Whirlpool, screen_axes}`, `craft`
  (Craft, Foil, HullSpec, Floating, Medium, CraftEvent), `ballistics` (Ballistics, Body, Placement, Shot, ShotEvent, V3,
  elevation_for), `sight` (Air, Band, Field, Murk, Obstacle, Seen, Target), `ui` (keybar, panel, tabs, Log, Table, Theme),
  `life` (body, mind, species, LifeEvent), `lang::Culture`, `display::Display`, `app::{run, Game}`, `view::inspect`,
  `prelude`. Any change here runs `cargo test --release` in `../Broadside` too.
- **Torpor** (`../Vampire-Wizard`): `rng`, `noise`, `lang::{Culture, hash_str}`, `weather::describe_wind`,
  `sim::light`, `world::FULL`, `prelude`.
- **Grimoire:** its own copy of `web/pterm.ts`; the palette in `src/draw/palette.rs` must stay in step with pterm's.
- **Behaviour to keep:** liquid in 64ths and its conservation, heat conservation across sleeping chunks, the ASCII fold
  in `display.rs`, draw-only-on-change in `app.rs`, `Materials::standard()` names, the text formats (`.atlas`, `.tbp`,
  `.tlib`, `.tpart`, `.tart`) byte for byte.

## 10. Open questions (also in `Queue/INBOX.md` as Decide items)

1. Which comes after the foundations: sound and NPC perception, or the region layer with rivers by erosion?
2. Physics tick: one physics tick is 1 game second for every game (Broadside would run two of its ticks per physics
   tick), or each game keeps its own tick and the world runs faster or slower to match?
3. How big should a world be: one generated area at a time (today), a moving bubble over an atlas (section 3.6), or
   several bubbles at once (two towns simulated together)?
4. Plans to schemas: what will you draw castle plans in: Tokamak art (glyphs), a plain text file, or an image?
5. Memory of places: should the player's map memory fade as in section 6, or stay as it is (seen once, kept forever)?
