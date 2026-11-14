# Simulation level of detail (tach-023)

Decision (Mickey, 11 Nov): the whole world is simulated to some degree, like Dwarf Fortress. The further from the player, the coarser the simulation, but **no level is ever off**. This note is design only; no engine change. It builds on `ROADMAP.md` 3.6 (the region layer and the bubble), `design/NPC_MINDS.md` (bodiless far tier, People registry) and `design/SKILLS_AND_TIME.md` (catch-up by an elapsed `Span`). Numbers marked (est) are guesses to be replaced by a bench.

## 1. The ladder

Distance is measured in atlas regions (`REGION` = 32 m) from the nearest **focus** (the player; later several foci, see section 5).

| Level | Where (est) | Unit | Ticked | What it is |
|---|---|---|---|---|
| L0 grid | the bubble: 3x3 to 5x5 regions (96-160 m) | 1 m cell, 16x16x1 chunks | every game tick (awake chunks only) | today's engine, unchanged |
| L1 column | next ring out, to ~16 regions (500 m) | 4 m x 4 m column, 3 vertical layers (ground, surface water, air) | once per game minute | aggregated cells, same quantities, no per-cell CAs |
| L2 region | out to ~100 regions (3 km) | one record per atlas cell (32 m) | once per game hour | totals and rates; the `src/region/` layer of ROADMAP 3.6 |
| L3 district | everything else | one record per 8x8 regions (256 m) | once per game day | statistics: populations, stocks, climate, influence; closed-form or ODE steps |

L3 is the floor and never stops. A world of 512x512 regions is 4096 L3 records, so there is no saving worth having from switching the far end off. Every region always has an L2 record, kept current even while it is inside the L0 bubble (the bubble writes its totals back on demotion, see 3).

## 2. What each level holds

- **L0:** everything `World` holds per cell (tile, material, fluid in 64ths, gas, heat, light, fire), things, and creatures as `Thing` + `Creature` with full bodies.
- **L1, per 4 m column:** liquid volume in 64ths-of-a-cell (integer) per fluid material; mean temperature by heat content; soil moisture; vegetation and fuel amount; a fire as a burning fraction with a spread front, not cells; creature groups (below). The terrain shape is *not* stored: it is regenerated from atlas + saved chunk diff.
- **L2, per region:** total surface water and groundwater (64ths), mean temperature and thermal mass, discharge across each edge (rivers), wind, rain, fuel, resources, population by species and faction (counts), named NPCs as individual records (NPC_MINDS far tier: place, duty, needs, aims), influence per belief.
- **L3, per district:** the same totals summed, plus births and deaths, trade, belief fields, regional climate, and an event queue for slow things (a plague, a migration, a war).

Creatures down the ladder: L0 full body. L1 *bodiless individual* (position, duty, needs, a wound summary; paths replaced by "arrive at the workplace at 08:00"). L2 **named** NPCs stay individual (they are plot); anonymous animals and crowds become counts with birth and death rates. L3 counts only. Named people are never merged into counts.

## 3. Moving between levels without losing anything

The rule: **every conserved quantity is an integer or a summed total that demotion computes exactly and promotion hands back exactly.** Shape (where in the region the water or heat sat) may be lost; totals may not. A promoted region then *settles* (a few hundred L0 ticks of fluid and heat relaxation, hidden behind the arrival or run as a short catch-up) so lost shape becomes believable shape.

**Liquid.** Demote L0 to L1: sum the 64ths of each fluid material per column (integer add, nothing rounds). L1 to L2: sum again. Promote: regenerate the terrain (atlas + chunk diff), then pour each column's volume in from the lowest cell upward, so ponds and river beds hold the water and slopes do not. The integer remainder goes to the lowest open cells (largest-remainder rule). Rivers are the one thing that flows across edges: L2 records discharge per edge, and a promoted bubble's edge uses the existing `sim::Source` springs and drains (worldgen already makes them) set to that discharge, and the bubble reports what actually crossed back to L2. Test: promote then demote with no ticks gives `total_fluid` equal to the start, and a full L0 -> L1 -> L2 -> L1 -> L0 trip does too.

**Heat.** Hold thermal *energy* (temperature x heat capacity summed over each cell's contents), not mean temperature, so mixing stone and air does not invent heat. Promote: lay down a base field (ambient from region climate, deep-earth gradient, material layers from the terrain) scaled so its energy equals the stored total; any remainder goes to the cells that held the most deviation. A fire is kept as fuel + burning fraction, so it re-ignites as cells. Test: round-trip energy equal within float rounding (1e-4 relative; the existing heat tests already treat 0.001 C as the floor).

**Population and bodies.** Demote: individuals become bodiless records (nothing lost), anonymous groups become counts. Promote: instantiate counts as bodies placed by the time-of-day routine (on duty at the workplace, sleepers in beds, wanderers near home) and bring each up to date with `Life::step` over the elapsed game seconds (needs and wounds already advance by `dt`). Count in = count out; deaths while away happen in the abstract model and are logged, so a number never drifts silently. Items and corpses go the same way (counts by kind, named items kept). Test: population equal across a round trip when the abstract model runs with zero birth and death rates.

**Hysteresis.** Promote at distance R, demote at R + 1 region, and hold a level for a minimum of a few game minutes, so a player pacing at a boundary does not thrash. Promotion is queued per chunk and budgeted per frame; the ring just outside the bubble is *pre-promoted* to L1 so L1 to L0 only adds detail.

**Time away.** A region demoted for a day is not frozen: its L1/L2 model ran the whole time, so promotion needs only the model state plus a settle, never a long replay. This answers SKILLS_AND_TIME open question 8 (how far the world runs while the player is away): all the way, at the level that region deserves.

## 4. Cost budget

The sandbox is ~4.2 ms/tick for 160x100x12, so against a tick budget of 8 ms, L0 takes about half and everything else must fit in the rest.

| Level | Work per update (est) | Cost per game second (est) |
|---|---|---|
| L0 | ~4 ms/tick at 160x100x12; scales with awake chunks | dominant |
| L1 | ~16k columns x 3 layers x ~100 ns = ~5 ms per game minute | ~0.1 ms |
| L2 | ~10k regions x ~200 ns = ~2 ms per game hour | ~0.001 ms |
| L3 | ~4k districts x ~1 us = ~4 ms per game day | negligible |

Promotion and demotion, per region (est, must be benched): demote ~0.1 ms (sums); promote ~1-5 ms for terrain regeneration plus the settle. L1 and L2 updates are spread over many ticks (a slice of columns each), never one spike. Rule of thumb: the whole far ladder costs under 15 percent of the L0 budget, or the radii shrink.

## 5. Atlas and moving bubble (option B)

- The atlas is the authored seed of L2: elevation, rock, biome, rivers, features. `src/region/` (ROADMAP 3.6) is L2, one record per atlas cell, ticked slowly, and the only copy of world state that outlives the bubble.
- Option B (a bubble moving over the atlas) is the L0 ring following the focus. When the player crosses a region edge the bubble re-centres by one region: the leading column of regions promotes L1 to L0, the trailing column demotes L0 to L1 and saves its chunk diff (terrain changes, placed things). Terrain is regenerated from atlas + diff, so L1 and L2 never store tiles.
- Several bubbles at once (two towns together, ROADMAP question 3) needs no new idea: each focus owns an L0 bubble and a region's level is set by its *nearest* focus. Costs add per bubble.
- Broadside is already on this ladder at sea: the `Sea` is a field (an L1/L2-style representation), only a ship's own `Scene` is L0, and distant ships are posed state plus bodiless crews.

## 6. Faster travel and promotion speed

Promotion must finish before the player reaches the edge of what is loaded. Walking is 1.4 m/s, so a 96 m bubble gives over a minute of warning. Speed shrinks that:

| Travel | Speed (est) | Time to cross one region | Stress |
|---|---|---|---|
| walking, running | 1.4-5 m/s | 6-23 s | none |
| horse, cart, ship under sail | 5-12 m/s | 3-6 s | needs prefetch ahead in the heading |
| Torpor speed spells; vehicles in the desert game (assumed); trains or aircraft in a spy game (assumed) | 20-100+ m/s | 0.3-1.6 s | about one region per second or faster; terrain regeneration becomes the limit |

Games that push this: **Torpor** first (the user's own example), the **desert wanderer** if it has vehicles, **Broadside** only mildly (ships are slow and the sea is already a field). The gothic RPG walks.

Mitigations, in order: (1) the L1 ring is wide enough that L1 to L0 is cheap (detail added, not terrain made); (2) prefetch biased by heading and speed, so the ring ahead is wider than the one behind; (3) promote in stages: terrain and structures first, creatures next, settle last, with the first frames drawn from L1 glyphs; (4) regenerate terrain off the main thread (it is pure); (5) a speed limit as a design knob: if a spell is faster than promotion can sustain, travel becomes a "skip" where the journey is resolved at L1 and only the destination is promoted, which the off-screen catch-up step already provides. Needs measuring first: the actual `worldgen::generate` time per region, before any radius is fixed.

## 7. Suggested order

1. Bench `worldgen::generate` per region and `World` memory per region (decides the radii).
2. Region layer (R1) holding L2 water, heat energy and counts, with conservation tests for the round trip with no engine tick in between.
3. L0 to L2 demote and L2 to L0 promote with settle, water and heat only; then creatures via the far tier.
4. L1 and L3 as separate steps once the L0/L2 pair works (L0 + L2 alone is a valid first version: a wider jump, simpler).
5. Fast-travel stress test: a scripted focus moving at 5, 20 and 60 m/s over a generated atlas, reporting stalls and conservation error.

## Questions for Mickey

- "whole world simulated to some degree": should events far away (a flood, a town burning) reach the player as news or rumour, or just be true when they arrive?
- Torpor's "speed spells": roughly what top speed do you imagine (a fast gallop, a few region crossings a minute, or near-instant)? It sets how wide the L1 ring and the prefetch must be.
