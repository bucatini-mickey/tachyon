# Tachyon: material x process interactions

Audit of what the engine does (or should do) when a material meets a process. Engine rule: it knows nothing about any game, and elements behave like the real thing (fire not underwater, wind moves smoke, water quenches fire, stone shelters from blasts).

Status: **missing** = not in the engine; **impl-untested** = code exists, no test; **tested-pass** = a test exists and passes; **tested-fail** = a test exists and fails. Audited 2026-10-31 by reading `src/sim/*`, `src/grid/material.rs`, and running `cargo test --release` (all suites green, so no row is tested-fail today). Rows marked "should have" are proposals for later tasks, not requests.

Test files are in `tests/`. Sim files are in `src/sim/`.

## Liquids

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Liquid falls and levels out | tested-pass | fluid.rs `step` | physics.rs `liquid_falls_and_levels` |
| Liquid mass conserved | tested-pass | fluid.rs | physics.rs `liquid_mass_is_conserved` |
| Pressure: U-tube comes level, still lake costs nothing | tested-pass | fluid.rs `pressure` | pressure.rs (3 tests) |
| Viscosity (oil, lava slower than water) | impl-untested | material.rs `Liquid{viscosity}`, fluid.rs | physics.rs adds an oil material but no test asserts its rate |
| Water + lava: obsidian + steam | tested-pass | fluid.rs `react` | physics.rs `water_on_lava_makes_obsidian_and_steam` |
| Immiscible liquids (oil on water, layering by density) | missing | fluid.rs | liquids "simply do not mix"; no density, oil does not float |
| Thin film evaporation | impl-untested | fluid.rs trickle, `Material::evaporate` | |
| Flow field carries floating things | tested-pass | fluid.rs `record` | vessel.rs `current_carries_a_raft_downstream` |
| Raft floats, paddled raft conserves water | tested-pass | grid/vessel.rs | vessel.rs |
| Springs and drains (rivers, sea edge) | tested-pass (indirect) | sim/mod.rs `Source` | worldbuild.rs river/sea; sea.rs; no direct unit test |
| Liquid pours through a hole broken by a blast | impl-untested | fluid.rs + projectile.rs `explode` | emergent; hull-leak tests are in vessel/sea layers only |
| Liquid soaks into porous ground (soil, sand absorb) | missing | | water lies on soil forever |
| Liquid flows through doors, grates | missing | | |

## Gases

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Smoke rises, diffuses, decays (faster under open sky) | tested-pass (partly) | gas.rs `step` | interactions.rs `smoke_rises_decays_and_pools_under_a_ceiling`; faster decay under open sky untested |
| Wind drifts gas under open sky | tested-pass | gas.rs | elements.rs `fog_drifts_downwind` |
| No wind indoors / under a roof | tested-pass | gas.rs, light.rs sky | elements.rs `no_rain_or_wind_under_a_roof` |
| Gas blocks sight above `opacity` | tested-pass (partly) | material.rs, sight | sight.rs `haze_fog_and_smoke_wash_her_out` (sea-scale only; per-cell opacity untested) |
| Gas conserved in diffusion | tested-pass | gas.rs | interactions.rs `gas_is_conserved_in_a_sealed_room` |
| Gas sealed by ceilings / pools under them | tested-pass | gas.rs | interactions.rs `smoke_rises_decays_and_pools_under_a_ceiling` |
| Steam from boiling water | tested-pass | heat.rs `phase` | physics.rs lava+water test checks steam |
| Steam condenses back to water on cold surfaces | missing | | no gas-to-liquid phase |
| Gas displaces / bubbles up through liquid | missing | gas.rs, fluid.rs | |
| Flammable gas (methane, fire damp) ignites and explodes | missing | | smoke is inert |
| Poison / suffocation by gas on creatures | missing | life/ | drowning is covered, gas is not |
| Fog forming from damp, cold air | missing | weather.rs | mist is a material, nothing makes it |

## Heat and phase change

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Conduction, symmetric, heat conserved | tested-pass | heat.rs `step` | physics.rs `heat_is_conserved` |
| Sleeping chunks do not leak heat | tested-pass | heat.rs, world wake | physics.rs `heat_does_not_leak_into_sleeping_chunks` |
| Hot air rises | impl-untested | heat.rs | |
| Sky drift to outdoor temp, deep-earth sink | impl-untested | heat.rs | |
| Ice melts to water | tested-pass | heat.rs `phase` | interactions.rs `ice_melts_to_water_when_warm` |
| Water freezes to ice | tested-pass | heat.rs `phase` | interactions.rs `water_freezes_to_ice_when_cold` |
| Water boils to steam, boiling soaks up heat | tested-pass (indirect) | heat.rs `phase` | via lava test |
| Lava holds its own temperature, glows | impl-untested | material.rs `self_temp`, `glow` | |
| Lava melts / ignites what it touches | tested-pass (partly) | heat.rs conduction + `ignite` | interactions.rs `lava_sets_adjacent_wood_alight`; melting untested |
| Rock melts to lava at high temperature | impl-untested | heat.rs `phase` (`melt`) | needs a material with `melt` set |
| Different conductors (metal vs wood vs air) | impl-untested | material.rs `conduct`, heat.rs `thermal` | |
| Frostbite / heat damage to creatures from ambient temperature | missing | life/ | burns exist (see Life) |
| Snow/ice from weather when cold | missing | weather.rs | |

## Fire

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Flammable wall burns, leaves ash, gives smoke | tested-pass | heat.rs `burn`, `burn_out` | physics.rs `wood_wall_burns_to_ash` |
| Fire spreads along wood | tested-pass | heat.rs `burn` | physics.rs `fire_spreads_along_a_wooden_wall` |
| Wind fans fire and pushes it downwind | tested-pass | heat.rs | elements.rs `wind_drives_a_fire_downwind` |
| Water in cell or beside it quenches fire, and it stays out | tested-pass | heat.rs `burn`, `quench` | elements.rs `water_puts_out_fire_and_it_stays_out` |
| Fire cannot start in a wet cell (>= 8/64) | tested-pass | heat.rs `burn` | interactions.rs `fire_cannot_start_in_a_wet_cell` |
| Rain quenches exposed fire, not sheltered | tested-pass (partly) | heat.rs `burn` | elements.rs `rain_puts_out_open_flames` (carried flames); burning cells under rain untested |
| Torch / lantern goes out underwater, hard to light wet | tested-pass | flames.rs | elements.rs `torch_goes_out_underwater_and_is_hard_to_light_wet` |
| Gale blows out exposed flames, not lanterns | tested-pass | flames.rs | elements.rs `gale_blows_out_torches_but_not_lanterns` |
| Wet things dry out, faster when warm | impl-untested | flames.rs | |
| Fire relights a dry flame | impl-untested | flames.rs | |
| Burning floor over empty space leaves a hole | tested-pass | heat.rs `burn_out` | interactions.rs `burning_floor_over_empty_space_leaves_a_hole` |
| Fire needs air (smothers when sealed, draught feeds it) | missing | | "no oxygen model" by design; a candidate if wanted |
| Fire jumps to flammable things (items, furniture, creatures) | missing | flames.rs / things | fire only lives in tiles; things only carry their own flame |
| Burning creature sets cells alight / runs to water | missing | life/ | |
| Fire on a liquid surface (oil burns on water) | missing | | blocked by lack of immiscible liquids |
| Dry vs green fuel (moisture state on tiles) | missing | material.rs | `spread` is per material only |

## Light

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Sky exposure spreads, leaks through doorways | tested-pass | light.rs `compute_sky` | interactions.rs `sky_light_leaks_through_a_doorway_and_fades` |
| Walls stop light, transparent walls (glass, ice) pass it | tested-pass (sky light) | light.rs, material.rs `transparent` | interactions.rs `glass_and_ice_roofs_pass_sky_light_but_stone_stops_it`; dynamic light untested |
| Dynamic coloured light from torches, fire, lava | impl-untested | light.rs `LightMap`, `gather_sources` | |
| FOV symmetric | tested-pass | grid sight | physics.rs `fov_is_symmetric` |
| Walls hide what is behind them | tested-pass (sea-scale) | sight | sight.rs `an_island_hides_what_is_behind_it` |
| Unlit or submerged flame gives no light | impl-untested | light.rs + flames.rs | |
| Flicker | impl-untested | light.rs `flicker` | |
| Water dims light with depth | missing | | |
| Smoke dims light | missing | | smoke blocks sight, not light |

## Weather

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Wind gusts and veers, deterministic | impl-untested | weather.rs | |
| Rain adds water to floors and puddles | impl-untested | sim/mod.rs `rain` | |
| No rain or wind under a roof | tested-pass | sim/mod.rs, open_sky | elements.rs `no_rain_or_wind_under_a_roof` |
| Rain washes blood | tested-pass | life | life.rs `bleeding_stains_the_floor_and_rain_washes_it` |
| Rain slants with wind | missing | sim/mod.rs `rain` | weather.rs doc says it slants; drops fall straight down |
| Wind pushes vessels | tested-pass | grid/vessel.rs, sea | sea.rs beam-wind test; vessel.rs has no wind case |
| Wind fans fire, drifts smoke, blows out flames | tested-pass | see Fire and Gases | |
| Rain soaks carried flames and puts out exposed ones | tested-pass | flames.rs | elements.rs |
| Snow, hail, lightning (lightning ignites trees, strikes high points) | missing | weather.rs | |
| Temperature from weather (cold rain, frost, seasons) | missing | weather.rs, heat.rs | outdoor temperature is one number |
| Rain fills puddles; rivers rise with runoff | missing | | no catchment |

## Life, growth and creatures

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Creatures drown, burn, fall | tested-pass | life/, movement.rs | life.rs `creatures_drown_burn_and_fall` |
| Swimming vs walking by liquid depth | impl-untested | movement.rs `SWIM_DEPTH` | indirect via raft/life tests |
| Pathing avoids hazards (fire, deep water, lava) | impl-untested | life/path.rs `hazard` | |
| Wounds: bruise, bleed, sever, fatal | tested-pass | life/body.rs | life.rs |
| Blood stains floor | tested-pass | life | life.rs |
| Hunger and thirst: find food and water | tested-pass | life/needs.rs | life.rs |
| Hunters kill prey and eat | tested-pass | life/ai.rs | life.rs |
| Plants grow (grass spreads, saplings, crops) | missing | | grass and trees are placed by worldgen only |
| Plants burn and regrow; ash fertilises | missing | | |
| Plants need water and light | missing | | |
| Wood rots when wet; metal rusts | missing | | |
| Corpses decay, attract scavengers, rot into soil | missing | | |
| Footprints in mud or snow | missing | | |
| Items float or sink; wet items; fire damage to items | missing | things.rs | only flames carry a wet state |
| Heat/cold stress on creatures | missing | life/ | |

## Projectiles and explosions

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Stone shelters wood from a blast | tested-pass | projectile.rs `shielding`, `explode` | elements.rs `stone_shelters_wood_from_a_blast` |
| Firebolt bursts at a wall, fizzles in water | tested-pass | projectile.rs `step` | elements.rs `firebolt_bursts_at_a_wall_and_fizzles_in_water` |
| Shell bursts on impact | tested-pass | projectile.rs | ballistics.rs `a_shell_bursts_on_impact` |
| Heavy ball penetrates oak, light one lodges | tested-pass | sea/ballistics.rs | ballistics.rs |
| Enough shot breaks a wall | tested-pass | projectile.rs / ballistics | ballistics.rs `enough_shot_breaks_a_wall` |
| Shot strikes people in its path | tested-pass | projectile.rs `pass` | ballistics.rs |
| Ball skips on water; plunging ball is spent in metres | tested-pass | sea/ballistics.rs | ballistics.rs |
| Blast sets flammables alight, makes smoke | tested-pass (ignition) | projectile.rs `explode` | interactions.rs `a_blast_sets_nearby_wood_alight`; smoke untested |
| Blast on water: spout, wave, no fire | missing | projectile.rs | |
| Blast in a sealed room: pressure, gas pushed out | missing | | |
| Wind deflects arrows and bolts | missing | projectile.rs | gale does not move shots |
| Fire arrow lights thatch; rain douses it in flight | missing | | |
| Projectile through glass breaks it | impl-untested | material.rs `strength`, `transparent` | |

## World generation

| Interaction | Status | Lives in | Test / note |
|---|---|---|---|
| Deterministic from atlas and seed | tested-pass | make/worldgen.rs | worldbuild.rs `generation_is_deterministic_and_regions_line_up` |
| Sea below sea level, held by springs and drains | tested-pass | worldgen.rs | worldbuild.rs `generated_island_has_sea_land_river_and_features` |
| Rivers flow downhill from a spring | tested-pass | worldgen.rs | same test |
| Biome surface (grass, sand, snow); steep slopes bare rock | impl-untested | worldgen.rs | only presence checked |
| Trees and marsh puddles per biome | impl-untested | worldgen.rs | |
| Ramps join every step | impl-untested | worldgen.rs | |
| Blueprints stamp structures | tested-pass | make/blueprint.rs | worldbuild.rs `blueprint_round_trip_and_stamp` |
| Generated terrain is stable under physics (no spurious flooding, collapse, fire) | missing | | no settle-and-compare test |
| Caves, ore veins, aquifers, lava layer | missing | worldgen.rs | |
| Temperature by altitude and biome | missing | worldgen.rs, heat.rs | |
| Soil moisture / water table | missing | | |

## Notes for later tasks

- No row is tested-fail (tach-002 added tests/interactions.rs, 10 rows now pass). Biggest untested clusters: light (all of it), gas decay and rise, phase changes other than water-lava, weather generators, worldgen detail.
- Cheapest high-value tests to write first: ice melt and water freeze, gas conservation, smoke rises and is sealed by a ceiling, blast ignites wood, burning floor under rain, sky light through a doorway.
- Biggest missing behaviours: immiscible liquids (oil), flammable gas, plant growth, condensation, lightning, porous ground.
