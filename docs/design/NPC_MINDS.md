# NPC minds: navigation, reactions, routines, relations, ambition, religion, dialogue (design proposal, tach-007)

Status: proposal only. No code has changed. Written against the engine as of 1 Nov 2026 (`src/life/{mind,ai,needs,path,mod,species}.rs`), the skills-and-time proposal (`docs/design/SKILLS_AND_TIME.md`, tach-006, whose `Span`, `Job` and `duration()` this builds on), and the gothic RPG's `plans/vision.md` and `plans/systems.md` (its `pall.rs` already has beliefs with a source and an acquaintance graph; this design generalises them).

Your brief (29 Oct): NPCs that navigate the world, react to the environment, have a wealth of dialogue that fits the character, and have ambition and religion. The gothic RPG adds: every NPC has a life, routine, personality, relations, children, drama and needs (Kingdom Come); a pantheon felt more than seen, with a balance that can slip; spy networks with handlers and double crosses; a reputation the player cannot change overnight. Constraints: Rust, crossterm only, hard ASCII, fast; no LLM at runtime.

## 1. The short version

1. **A mind is layers, fastest first.** Reflex (fire, threats: exists today) > needs (exists) > job and routine (tach-006 jobs, existing routines) > agenda (ambition, faith, missions: new, ticks once a game day) > values (personality and creed: new, nearly static). A lower layer can always pre-empt a higher one, as `urgent()` already pre-empts `plan()`.
2. **Three levels of detail.** NPCs near the player run `ai::act` per tick as now. NPCs in sleeping chunks run no AI at all: their position is a pure function of their routine and the clock, worked out when the chunk wakes. People in other regions have no body, only a record, ticked once a day. That is how a whole island of people stays cheap.
3. **A `Person` outlives its body.** A new `People` registry holds who someone is (name, culture, household, personality, ties, knowledge, faith, aims) under a stable `PersonId`. A `Creature` is the body on the map and points to its person. The dead are still remembered; a merchant in another town still schemes.
4. **Everything social is facts.** One small `Fact` type ("Tomas killed a sailor at Varnakkel, day 41") held by people with a confidence and a source. Witnessing creates facts, talking spreads them, letters carry them, lies are facts marked false. Reputation, rumours, spies, heresy hunts and dialogue topics all read the same table.
5. **Navigation goes hierarchical.** Keep today's A* for the last few dozen metres; add a place graph (houses, smithy, chapel, road junctions, authored in Tokamak) and a chunk-portal graph for long trips, and shared "Dijkstra maps" for crowds going to one place or fleeing fire.
6. **Reactions come from a stimulus bus.** Every `LifeEvent` and world event (fire, a scream, a theft, a loud spell) becomes a stimulus in a per-chunk bucket with a loudness. NPCs read the buckets near them on their think tick and look up a data table: (what happened, who to, my personality) gives (ignore, look, investigate, shout, flee, fight, help, report, gossip).
7. **Ambition and faith are data the game registers**, like species and skills: aims with schemes (short step lists), creeds with values, taboos and rites, powers (gods) with an influence field over the map. The engine runs the machinery; the gothic RPG supplies the nine gods, the conquistadors' greed and the Templars.
8. **Dialogue is a rule-matched line bank, not generated text.** Text files of lines, each with criteria (who speaks, to whom, mood, weather, what they know); the most specific matching line wins (Valve's "dynamic dialogue" method from Left 4 Dead). Slots, registers (coarse, courtly, pious ...), oaths from the speaker's creed, words from their generated language and hedged rumours ("they say ...") multiply a few thousand authored lines into hundreds of thousands of distinct ones. Choosing a line costs microseconds and happens only when someone is in earshot.

## 2. What `life/` has today

| Where | What it does | What it lacks for this brief |
|---|---|---|
| `mind.rs` `Mind` | control, routine, home, `faction: u16`, current `Goal`, path, grudges, asleep, timers (`next_act`, `next_think`, `next_plan`), `alarmed`, `stuck`, no-food/no-water back-offs | identity beyond the body, personality per individual, memory, relations other than anger, mood |
| `mind.rs` `Routine` | `Duty { from, to, task }` in hours, wraps midnight, first match wins; `Task` = Sleep / Work / Wander / Stay | days of the week, holy days, "go to any tavern", jobs inside a duty (tach-006 section 8), per-NPC jitter |
| `mind.rs` `Grudge` | anger at a `ThingId`, cools by 1 per 30 game minutes (`life/mod.rs`), last seen position | liking, trust, fear, kinship; anything that lasts more than an hour |
| `ai.rs` `act` | every `URGENT_EVERY` (6) ticks: `urgent()` (escape heat, threats by grudge/prey/wariness with nerve vs odds, hunting); every `PLAN_EVERY` (60) or when the goal ends: `plan()` (needs scored `thirst^2 x 2`, `hunger^2 x 1.6`, fatigue; then routine; then idling near home) | perception of anything but creatures; hearing; reactions to events (a corpse, a theft, a fire next door); helping |
| `ai.rs` `look_around` | scans **every** creature, line-of-sight with `sees()` (Bresenham through `opaque_idx` and `blocks_sight`) | O(N^2): fine for a sandbox, not for a town of 500. `find_food` also scans every thing in the world |
| `needs.rs` | hunger, thirst, fatigue, core temperature, wet, breath; `factor()`, `fatal()`, `words()` | social and spiritual drives (company, devotion, leisure) |
| `path.rs` | A* (`BinaryHeap` + `HashMap`), 10/14 step costs, x3 swimming, stairs, hazard avoidance, budget 4000 cells; doors count as open | long trips (a 12 km road is 12 000 cells, three times the budget), locked or private doors, route reuse, crowds, knowing the way |
| `species.rs` | `Temperament { aggression, courage, loyalty }` per species, diet, prey, `wary_of`, nocturnal, `walk` ticks per step | temperament per individual |
| `mod.rs` | `LifeEvent` stream (struck, fell, burned, died, ate, slept ...), `narrate()`, `name()` | events from the world (fire started, door forced, item taken) |

The good bones: the event stream is a ready-made perception feed; `urgent`/`plan`/`set_goal` is already a layered utility AI with hysteresis (`set_goal` keeps a goal of the same kind, so NPCs do not dither); needs already advance by game seconds (`dt`), so catching up a sleeping NPC is just a bigger `dt`; paths are reversed `Vec<Pos>` with re-planning when they stop fitting. None of this needs rewriting, only extending.

## 3. Principles

- **Engine knows no game.** Trait names, fact predicates, creeds, gods, aims and lines are registered by the game, like `Life::add_species` and tach-006's `add_skill`. The engine ships an optional starter set (like `Materials::standard()`, `species::human()`), enough for the sandbox.
- **Numbers inside, words at the edge.** Minds are small floats, small integers and bit sets. Text appears only when a line is chosen for someone in earshot.
- **Time is fractions of a day.** Routines, aims, gossip rates, devotion, memory fading all use `Span` (tach-006). Physics stays in seconds.
- **Deterministic.** Randomness comes from `Life::rng` or from hashing (person, day, topic). The same seed replays the same town; the gothic RPG wants occult effects learnable, and the same goes for who gossips to whom.
- **Bounded.** Every per-person list has a cap (ties 24, facts 48, aims 3, active schemes 1). Every per-region process has a budget. Nothing grows without limit over a 100-day game.
- **The sim never lies; drawing and speech may.** The gothic Veil rule (`plans/systems.md`) applies: a fact has a truth flag the NPCs cannot see; a hallucinated line or glyph is a display substitution, never a change to the world.

## 4. The layered mind

```
            once a day            values    personality, creed, culture (near static)
                 |                agenda    aims and schemes, faith duties, missions
   when a goal ends / 60 ticks    routine   duty for this hour -> a Job (tach-006) or a Goal
                 |                needs     thirst, hunger, sleep, warmth + drives
          every 6 ticks           reflex    fire, threats, stimuli worth reacting to
                 |
           every act tick         pursue    one step of the current Goal (exists)
```

The agenda writes into the routine (an extra `Duty` for today: "19:00 to 21:00, the tavern, meet Ines"), so ambition shows up as visible behaviour the player can watch and interrupt, and the existing `plan()` carries it out with no new decision code. The reflex layer reads stimuli (section 6) in addition to the threats it already handles.

Proposed new files (all inside `life/`, the "Tachyon engine setup" session's area; message it first):

```
life/person.rs     PersonId, People registry, Persona (traits), Mood, households
life/social.rs     Tie, Faction (generalises faction: u16), regard rules
life/fact.rs       Fact, Ledger (shared facts), Knowledge (per person), gossip, reputation
life/stimulus.rs   Stimulus, per-chunk buckets, reaction table
life/nav.rs        Place registry, chunk portal graph, Dijkstra maps, access rules
life/agenda.rs     Aim, Scheme, daily agenda tick, the director budget
life/faith.rs      Creed, Power (gods), influence fields, pacts
life/talk.rs       line bank loader, rule matcher, slot expansion, barks, topics
```

## 5. Individuals: persona and mood

**Temperament per creature.** `Species::temperament` stays as the default; each `Creature` gets its own copy, set at birth from the species default plus a seeded jitter (`+-0.15`). One line in `Life::add`; `urgent()` reads the creature's copy instead of the species'. Animals get personality for free (a brave dog, a skittish one).

**Persona: game-registered trait axes.** Like skills: the game registers `TraitId`s and each person has a `Vec<i8>` of values -100..100 (an `i8` is a byte; 16 traits = 16 bytes). Suggested starter set: `piety, greed, honesty, sociability, curiosity, ambition, cruelty, humour, patience, pride, lust, nerve`. The engine reads none of them by name; data tables (reactions, aims, lines) refer to them by id. Rust note: `TraitId(u8)` is a **newtype**, so you cannot index persona with a skill id by mistake.

**The gods' axes are trait axes.** Your four balanced gods map straight onto two persona axes: `calm..passion` (tranquil x vs chaotic z) and `mercy..judgement` (chivalrous y vs punishing w). A person's position on them is their natural religious affinity, so who converts to what, and who a god's influence sways, falls out of numbers that also drive their everyday behaviour (section 13).

**Mood** is four floats that move fast and decay to the persona's baseline: `fear`, `anger` (the grudges stay as anger at someone; mood anger is the general temper), `joy`, `stress`. Stimuli push them; a meal, sleep, company and prayer pull them back. Mood feeds three things: nerve in `urgent()`, which reactions are chosen, and which lines are said ("Not now." vs "Ah, come in, come in!").

**Seeds and authoring.** A generated person's persona is `hash(world_seed, person_id)`, so the same town comes back the same. Tokamak can pin any value for an authored character (the spymaster is patient and dishonest, whatever the hash says).

## 6. Perception and reactions to the environment

### 6.1 Stimuli

```
Stimulus { kind: StimKind (u16, game-registered), at: Pos, tick: u64,
           by: Option<ThingId>, to: Option<ThingId>, loud: u8 (cells), seen: u8 (cells), weight: u8 }
```

- **Sources:** every `LifeEvent` (struck, died, burned, collapsed ...) is turned into one by `Life::step`; world systems push their own: fire started (from `sim/flames`), explosion (`projectile::explode`), door forced, item taken from an owned place, a loud spell (gothic `loudness()`), a bell rung, a shout (NPCs emit `Shout` stimuli themselves, so an alarm spreads street by street).
- **Storage:** a small ring buffer per 16x16 chunk (8 entries, oldest dropped), kept for a minute of game time. Rust note: a fixed-size array plus a cursor; no allocation per event.
- **Reading:** on its think tick (every 6 ticks, as now) an NPC reads the buckets of the chunks within hearing (at most the 3x3 around it, for `loud` up to 16 cells). Hearing: `loud` minus 4 per wall on the Bresenham line, minus 8 for a closed door. Sight: `seen` within range and `sees()`. The cost is O(stimuli nearby), not O(creatures in the world).
- **Fix while there:** `look_around` should use a creature-by-chunk index instead of scanning all creatures (`Things` has `at(pos)` per cell but no radius query; add `Things::near(pos, r)` over chunk buckets). `find_food` gets the same treatment.

### 6.2 The reaction table

A data table, read top to bottom, first match wins:

```
stimulus     | relation to by/to          | condition                      | response
-------------+----------------------------+--------------------------------+-----------------------------
Died         | to = kin or friend         | -                              | Mourn + resent(by) + Report
Struck       | to = same faction          | nerve > 0.5                    | Fight(by)   (today's loyalty)
Struck       | to = anyone                | guard role                     | Shout + Fight(by)
Struck       | -                          | nerve < 0.3                    | Flee(by) + Report
FireStarted  | at = own home or work      | -                              | Shout + Help(fetch water)
FireStarted  | -                          | curiosity > 0                  | Look
Taken        | at = own place             | saw it                         | Shout(thief) + Report
Corpse       | -                          | -                              | Investigate + fear + Report
LoudSpell    | creed forbids its root     | -                              | Shout(heretic) + Report
Shout        | -                          | -                              | Look, then the shout's table
Rain         | outdoors, not field work   | wet > 0.3                      | Shelter (nearest roofed place)
Dark         | outdoors, no light         | -                              | GoHome or carry light
```

Responses are a small enum that `urgent()` turns into a `Goal`: two new goals, `Investigate(Pos)` (walk there, stay a while, record what is seen) and `Help(Job)` (a tach-006 job: carry water, drag the wounded); `Report` and `Mourn` are not goals but writes to knowledge and mood (section 9). The table is per game; the engine ships a minimal one for the sandbox.

### 6.3 Environment, not just events

The world already has the numbers; NPCs only need to look at them when they plan:

- **Rain and cold:** `needs.wet` and `needs.temp` exist. `plan()` adds a `Shelter` option scored like a need: go to the nearest roofed place (place graph, section 7) or the hearth.
- **Dark:** `Clock::daylight()` and the light sim: outside at night without a lamp, people go home earlier, and guards light torches (a held light thing).
- **Fire next door:** a bucket chain is just several NPCs with `Help(fetch water)` jobs; the fluid sim makes the water real and the fire really goes out (or doesn't).
- **Blood and bodies:** `Stains` and corpse things are seen (`seen` radius) and feed the `Corpse` stimulus.
- **Witnesses matter:** a crime only becomes a fact in someone's head if a stimulus reached them. Theft in an empty street is unknown until the owner notices the gap (a `Taken` stimulus with `by: None`, then suspicion, section 14).

## 7. Navigation

The A* in `path.rs` stays as the street-level router. Around it:

**7.1 Places.** A `Place { kind, anchor, bounds (rects), owner (household or faction), access, spots }` registry, authored in Tokamak (blueprint markers: "bed", "anvil", "pew", "counter", "stool") or derived from furniture flags (`bed()`, `stocked()`). Routines and aims name places, not coordinates: `Task::Visit { kind: "tavern" }` picks the nearest one the NPC knows. Spots are reserved, so two people do not sit on one stool and the smith's anvil is his.

**7.2 The chunk portal graph (long trips).** For each 16x16x1 chunk, the gaps in its border that a creature can cross are "portals"; a portal graph links portals within a chunk (one small A* each, cached) and across chunks. A 12 km road crosses about 750 chunks, so the portal graph has a few thousand nodes and a route over it costs a fraction of a millisecond; the NPC then plans the detailed path only one chunk ahead as it walks. A chunk's portals are rebuilt when a tile in it changes passability (a `topo_version` per chunk bumped by `World` when a wall or floor changes). This is HPA* (Botea, Mueller and Schaeffer, 2004); the existing `neighbours()` gives the costs, so terrain and swimming costs carry over.

**7.3 Shared maps (crowds and fleeing).** A Dijkstra map (distance from every cell in an area to a goal) is computed once and shared by everyone going there: the market on market day, the chapel at mass, the town gate at a fire. Fleeing uses the Brogue trick: multiply a "distance to danger" map by -1.2 and roll downhill, so panicked people run round corners and out of doors instead of into the wall that `Flee` picks today. Cached per (place, level), dropped after an hour of game time unused.

**7.4 Access and doors.** `search()` gains an `Access` argument (who is walking: person, household, factions, keys held). Locked doors are walls unless you hold the key; private places cost x4 for strangers (they go round unless there is no other way) and entering one emits a `Trespass` stimulus for whoever can see. Today doors "count as open", which is right for the owners and wrong for burglars.

**7.5 Knowing the way.** Each person has a bit set of places they know (their town, their trade's places, the roads they have travelled). Routes only use known places as targets; a stranger asks for directions (a dialogue topic, section 15.6). Optional per game; Broadside does not need it.

**7.6 Crowds.** When two walkers want each other's cell, they swap (DF does this); a walker reserves its next cell for a tick. Today's "stuck 3 times, step at random" stays as the fallback.

**7.7 Off-screen travel.** In a sleeping chunk an NPC on a trip is a travel `Job` (tach-006 section 6.1): duration from the portal route length and speed; its position is interpolated along the portal route only when someone needs it (a chunk wakes, the player asks where X is).

**7.8 Taxis and stations.** Your "no plain fast travel" (Morrowind's silt striders): a station place with a timetable is an edge in the place graph with cost "wait + ride". NPCs plan with it exactly as with walking; the player and an NPC can ride the same coach.

## 8. Routines, households, jobs

- **Duties stay in hours** for authoring (`Duty { from: 8.0, to: 17.0 }`), read through `Clock::day_fraction() x 24` (tach-006), so they follow the day length.
- **New task kinds:** `Task::Job { spec, at }` (a loop of jobs at a place: the smith forges what the forge has), `Task::Visit { kind }` (any tavern), `Task::Attend { event }` (mass, market, a hanging), `Task::Rite { creed }` (section 13).
- **Calendar filters:** a duty can say `days: weekdays`, `holy days only`, `market day`. Needs a week and holy-day calendar on the clock (tach-006 open question 9).
- **Jitter:** each person shifts every duty by a seeded few minutes, so a town does not get up in lockstep.
- **Templates by role:** smith, farmer, guard, priest, child, beggar, sailor, written once (in Tokamak or a text file) and given to many; individuals override.
- **Households:** a household is a `Faction` of kind `Household`: shared home, beds, food store, purse; parents and children; meals together at a table (an `Attend` duty), which is also the main time family gossip and family barks happen.
- **Ownership:** things get an optional owner (household or faction). NPCs eat from their household's store or buy; `find_food` today eats whatever is near, which in a town is theft. A hungry, dishonest person may steal anyway, and that is a `Taken` stimulus: emergent crime from existing needs.
- **Where should X be now?** `Routine::where_at(day_fraction) -> (Place, spot)` is a pure function. When a chunk wakes after the player has been away, its people are placed at their duty spot or part-way along the portal route between two duties, needs are advanced by the elapsed `dt` (already supported), and jobs advanced by the elapsed `Span`. No ticks are simulated for the hours in between. This is the main trick that makes a living town cheap.

## 9. Relations, knowledge, memory

### 9.1 People and ties

```
Person { id: PersonId, name, culture (make::lang Culture), body: Option<ThingId>, household,
         persona: Vec<i8>, mood, ties: Vec<Tie> (cap 24), knows: Knowledge, faith: Faith,
         aims: Vec<AimState> (cap 3), born: day, alive }
Tie    { who: PersonId, kind: TieKind (bit set: kin, spouse, parent, child, friend, lover,
         rival, master, servant, handler, agent ...), regard: i8, trust: i8, fear: i8,
         known: u8 (familiarity), met: day }
```

- About 1.5 KB per person; 2 000 people on an island is about 3 MB.
- Rust note: `TieKind` as a bit set is the same idea as the existing `Does` flags: one `u16` where each bit is a yes/no.
- **People outside the 24 ties** are judged by stereotype: their factions' regard for each other, the viewer's persona against the stranger's culture and class, and reputation (section 10). That keeps the list short and still lets prejudice be a modelled thing (handled with care: a measured, visible cause and effect, not a slur generator).
- **Regard rules (data):** helped me +, gave a gift +, hurt my kin --, insulted me -, prayed with me + (same creed), betrayed me -- and trust --. Scaled by persona (proud people take insults harder). Ties drift back toward a baseline over weeks; kinship never drops below a floor.
- **Factions generalise `faction: u16`:** `Faction { kind (household, guild, house, garrison, cult, network, crew), name, ranks, members, regard: per other faction, secret: bool }`. "Same faction defends each other" keeps working; guilds, the Templars and spy networks are factions with different flags.
- **Grudges stay** as the short hot anger they are now; a grudge that recurs raises the long-term tie's fear or lowers its regard.

### 9.2 Facts and knowledge

```
Fact      { subj: Ref, pred: PredId (game-registered), obj: Ref, at: Option<PlaceId>, day: u32, true_: bool }
Ref       = Person | Faction | Place | Thing kind | Power (a god) | Number
Knowledge = Vec<Held> (cap 48)      Held { fact: FactId, conf: u8, source: Source, heard: day }
Source    = Saw | ToldBy(PersonId) | Read(ThingId) | Inferred | Dreamt
```

- **One ledger, many holders.** Each fact lives once in `Ledger` (a `Vec<Fact>`, ids are indices); people hold ids with their own confidence. 48 holdings x 8 bytes per person.
- **Truth is in the fact, not in the holder.** The sim knows a fact is false; nobody in the world does. This is `pall.rs`'s `Belief { subject, claim, conf, source }`, generalised, and pall's gaslighting is "add a false fact to someone's knowledge with your trust as its confidence".
- **Created by:** witnessing (a stimulus that reached the person), being told, reading (letters, ledgers, wanted posters and holy books are things that carry facts), inference rules (saw X leave the house where the body was found -> "X killed Y", low confidence).
- **Gossip:** when two people talk (shared meals, the tavern, a market stall, an NPC-NPC conversation), each may pass one or two facts, chosen by juiciness (a per-predicate weight: murder high, weather low) x sociability x trust between them. Confidence drops a step per hop (`x 0.8`); a fact heard from three sources rises. Off-screen this runs as one pass an hour over the town's tie graph (like pall's nightly spread), costing a few microseconds per person.
- **Forgetting:** low-juice, low-confidence facts are evicted first when the list is full; old facts fade unless they touch the person's kin or faith.
- **Memory of places** (7.5) is the same idea with a bit set. The player's fog-of-war memory (tachyon.md, rendering notes) is a sibling system and out of scope here.

## 10. Reputation (the player and everyone else)

Your requirement: dress, demeanour and deeds all count, and the player cannot remake themselves overnight. Three parts, each a vector over game-registered axes (suggested: `violence, crime, piety, learning, wealth, occult, honour`):

1. **Deeds: what people have heard.** The facts about you that a community holds, summed per axis (`conf x predicate weight`). Local: Varnakkel has heard about the murder, the next valley has not, until gossip or a traveller carries it. Cached per community and refreshed when the gossip pass runs.
2. **Character: who you have become.** A slow moving average (half-life about two game weeks, a `Span`) of what you actually do: skills used (tach-006 learning already counts use), deeds done, company kept, rites attended. You cannot change it by changing clothes.
3. **Appearance: how you look right now.** Tags from worn things (heavy armour, a scholar's gown, a hood, holy symbol), state (blood on you: the stain and wound systems already know, wounded, drunk, wet and filthy), weapon in hand, and posture (sneaking, running).

**Impression** of you in one NPC's eyes = `a x appearance + c x character + d x deeds`, where the weights depend on how well they know you (`Tie::known`): a stranger goes mostly by appearance, an acquaintance by character and what they have heard, a friend by your shared history. Bloody armour frightens a stranger; it does not make your old landlady forget you were a meek scholar. The impression picks a **stance** (warm, respectful, wary, intimidated, contemptuous, shunning) used by dialogue and by checks: persuasion uses your skill plus learning and honour; intimidation uses your skill plus violence and appearance (so an intellectual is more convincing and less frightening, exactly as in your note).

NPCs have reputations by the same machinery, so the gossip about the blacksmith's debts uses the same code and the same lines.

## 11. Ambition

**Aims (game data).** `Aim { name, wants: trait weights (who takes it up), measure (how progress is read from the world: coin, rank, a tie, a fact), schemes }`. Starter set: wealth, rank in my faction, revenge on X, marry X, master a skill, a secret (forbidden lore), a seat (council, guild master), holiness, protect my family, flee this place.

**Schemes** are short step lists (an HTN reduced to its bones): each step has a precondition and an action that the agenda turns into today's duty, a job, a conversation or a fact. Example, `rank`:

```
scheme climb_honestly  : work extra hours (add duty) ; gift the master (job + regard) ; ask for promotion (talk)
scheme climb_dirty     : learn the rival's secret (gather facts) ; let it slip (gossip a true fact)
scheme climb_foul      : spread a lie about the rival (false fact) ; or, if cruelty > 60 and the rival is
                         alone with no witness near, remove the rival (Attack)
```

The scheme a person picks follows their persona (honesty, cruelty, patience) and the means at hand. A conquistador-like burgher with high greed and pride and low honesty picks the dirty schemes; a patient bird-people elder may hold one aim for years and never touch a scheme that harms.

**The agenda tick** runs once a game day per person (in any LOD): re-score aims, advance one step of the active scheme, write tomorrow's extra duty. A few microseconds each.

**Rivalry is automatic:** two people with the same seat as their aim get a `rival` tie. Most drama then generates itself from schemes colliding.

**The director.** A per-region budget of dramatic acts per week (murders, elopements, arrests, house fires) stops a town tearing itself apart in a month, RimWorld-style. Your knob: a quiet farming valley gets a small budget, the conquistador port a big one.

**Life events** (courtship, marriage, birth, a feud, an affair, a death in the family) are "storylets": rules with preconditions over ties and persona, rolled rarely (once a game week per household). A birth makes a new `Person` with a child routine; children age into role templates.

## 12. Drives (needs beyond the body)

The physical needs stay as they are. Add optional game-registered **drives** in the same style: `company`, `leisure`, `devotion`, `safety`, plus game ones (the gothic Veil's obsessions are a drive with one specific satisfier: count the bells). Each rises at a rate per day fraction scaled by persona (a sociable person needs company sooner), and is satisfied by activities tagged with it (the tavern: company, leisure; the chapel: devotion, company). In `plan()` they are scored after thirst, hunger and sleep, before the routine's idle slots, so on a free evening a person goes where their drives point. This is how a pious person ends up at vespers and a lonely one at the tavern with no routine written for either.

## 13. Religion (the gothic pantheon)

### 13.1 Engine: creeds, powers, faith

- `Power { name, axis: (calm..passion, mercy..judgement) position, counters: Option<PowerId>, domains: [pred ids] }`. The gothic RPG registers nine; x, y, z, w with counter pairs x/z and y/w; four more when you have them; the Allfather.
- `Creed { name, powers (one, a pair, the whole pantheon), values (trait weights it approves), taboos (fact predicates it condemns), rites (duties: when, where, what), clergy (a faction), holy places (place tags), oath words (for dialogue) }`. A creed is not a god: a heresy, a cult and an orthodox church can all honour z differently.
- Per person: `Faith { creed, devotion: 0..1, doubt: 0..1, secret: bool, pacts: Vec<Pact> }`. Devotion is a drive (section 12); rites are duties; taboo facts held about someone lower your regard for them and may become a `Report`.

### 13.2 Gods felt, not seen: influence fields

Each power has an **influence field**: one number per district (a coarse grid, about 64x64 cells, a few hundred values for an island), updated once a game day:

- **Rises** with devotion in the district (prayers at its holy places) and with deeds in its domain: quarrels, passion, broken vows feed z; harmony, quiet labour, growing things feed x; rescues and kept oaths feed y; punishments carried out, confessions and executions feed w.
- **Pushes against its counter:** `dx -= k x z` and `dz -= k x x`, so x and z, y and w balance. With mild positive feedback (more z means shorter tempers, means more quarrels, means more z) the balance can slip, sometimes hard, as you describe, until the counter-god's faithful respond (their own aims gain "restore the balance" schemes).
- **The Allfather** is a slow restoring term pulling every district back to balance. Alive and distant: a weak pull. Dead: no pull at all, and slips run further. One number that answers your open question in play-testable form.

**Felt as multipliers on numbers that already exist**, never as a visible god:
- z-heavy district: persona drift toward passion and anger, reaction thresholds lower (more fights from the same events), ambition schemes advance faster, omens (crows, a cold wind from the weather sim, milk turning: text events).
- x-heavy: fights rarer, plants grow faster (when growth exists), ambitions stall, people linger at work and forget grudges sooner. Calm, and a little dead.
- y-heavy: guards braver (nerve +), strangers helped, oaths kept.
- w-heavy: crimes reported more, punishments harsher, the Templars more active, confessions.

This fits `plans/systems.md` ("deities are weather: each has a territory, appetite and drift").

### 13.3 Cults, pacts, Templars, conversion

- **Cults** are secret factions with their own creed. Rites are Tongue sentences (systems.md); the engine sees a rite duty with a text. Overhearing one is a stimulus that gives the hearer a fact ("X is of the cult") and, in the gothic RPG, the root.
- **Pacts:** `Pact { power, gift, price }`; the price is an obsession drive or a scheme the power's aim imposes (a pact with z gives you an ambition you did not have).
- **Templars:** a martial faction whose creed taboos the occult. Their aim "purge heresy" runs the same report-and-investigate pipeline as the spy networks (section 14): they hunt facts like "X worships the forbidden", then people.
- **Conversion and doubt:** persuasion conversations, witnessed miracles and atrocities (facts) and a district's influence move devotion and doubt; past a doubt threshold a person may switch creed, keeping it secret if their household would not approve.

## 14. Spy networks (the hardest case)

No new machinery: a network is factions + facts + ties + agenda + dialogue.

- **Structure:** a faction with `secret: true` and cells. Membership is itself a fact ("Ines is of the Red Ledger"), and each member holds only the membership facts for their handler and cell. Uncovering a spy reveals only what they knew.
- **Intelligence is facts.** A spymaster's aims are "know about faction F / person P". Schemes assign tasks to agents: watch a place (a `Stay` duty, with every stimulus seen there becoming a held fact), follow a person (a `Go` that re-targets each plan), befriend someone (raise trust so gossip gives up their facts), steal a letter (letters are things carrying facts), plant a lie (a false fact).
- **Reporting:** a meeting duty with the handler or a dead drop (a place; a letter left in it). Facts move up; the handler merges confidences.
- **Turning and double crosses:** an agent's loyalty is their regard and trust for the handler plus their regard for the faction, against what a rival network (or the player) offers: coin, a threat (a fact used as blackmail), creed, revenge. A turned agent keeps reporting but sends selected or false facts (source and truth flags carry this exactly).
- **Detection:** handlers compare reports from several sources; contradictions raise suspicion of the source (pall's "neighbours compare notes"). Suspicion is a fact too ("Ines lies"), and past a threshold the network's scheme against her starts.
- **The player as an agent:** reports are dialogue choices that hand facts over; the player can lie (a false fact under their name) and a later contradiction catches up with them. Missions are scheme steps addressed to the player instead of an NPC. The player is "an agent amongst many larger characters" (your Kingdom Come note) because the NPC agents run the same schemes in parallel.
- **Cost:** a network is tens of people and a daily tick; facts are capped. Negligible.
- **Delicacy:** the system carries facts about slavery, massacres and betrayals the same as any other; tone is in the lines and the authored world, which stay yours.

## 15. Dialogue: a wealth of lines, cheap at runtime

### 15.1 Three kinds of speech

1. **Barks:** one-liners from state and stimuli: greetings, weather, needs, reactions ("Fire! The mill's afire!"), working mutters, prayers. Shown in the message log or above the speaker.
2. **Conversation with the player:** topics (Morrowind's keyword list), stance from impression, checks (persuade, intimidate, bribe, lie) using skills and tach-006 durations (a persuasion takes minutes, an argument longer).
3. **NPC to NPC:** almost always abstract (facts move, regard changes). Rendered as a short exchange of barks only when the player is in earshot.

### 15.2 The line bank and rule matching

Lines live in plain text files under the game's `dialogue/` folder (moddable, diffable, ASCII-checked). Each line names a **concept** (what kind of thing is being said), a list of **criteria**, an optional **weight**, and the text:

```
concept  greet
when     listener=player  stance=warm  time=morning  speaker.humour>40
say      "Morning, {address}! Sun's up, and so's the price of bread."

concept  greet
when     listener=player  stance=wary  listener.app.blood>50
say      "{address}. You've blood on you. Whose?"

concept  greet
when     listener=player  speaker.register=courtly  listener.deeds.honour>60
say      "Your servant, {address}. Your name came before you, and kindly."
weight   2
```

At runtime the speaker builds a **query**: a small list of key/value pairs about speaker, listener and context (persona, mood, stance, time, weather, place kind, needs words, what the speaker is doing, recent stimuli). The matcher looks only at the rules for that concept, keeps those whose criteria all hold, and picks among the ones with the **most criteria** (the most specific line wins), weighted at random among ties, skipping lines this speaker said recently. This is Valve's rule-based response system (Elan Ruskin, "AI-driven Dynamic Dialog", GDC 2012), which gave Left 4 Dead its variety with a few thousand lines.

Implementation: the files are parsed once at startup into rules sorted by concept, criteria as `(key: u16, min: i16, max: i16)` triples, texts in one big string with offsets (a "string arena": one allocation instead of thousands). A selection checks a few dozen rules of a few criteria each: microseconds. Nothing runs for NPCs nobody can hear.

### 15.3 Voice: what makes a line fit the character

- **Register:** each person has a speech register chosen from culture, class and persona: `plain, coarse, courtly, pious, scholarly, clipped (soldier), florid (merchant), archaic (bird people)`. Lines are tagged with registers; a concept has variants per register. Five registers turn "greet" into five different people.
- **Address forms and oaths:** `{address}` resolves from stance and rank ("sir", "stranger", "friend", "you", "Your Grace"); `{oath}` comes from the speaker's creed ("By the Still Root", "Judge's teeth!", "Saints keep us"), with the creed's oath words registered in `Creed`.
- **Personal tics:** each person gets two or three seeded tics from a list (a pet word, a sigh "Ah well.", a habit of ending on a question, a stammer on fear): applied as small text transforms.
- **Their own language:** `make::lang` already gives every culture a lexicon. `{native:salt}` puts the speaker's word for salt in the line; bird people use honorifics in their tongue; a merchant from the east swears in his. Names in lines are real generated names with translations, so "Varnakkel" can be glossed "Saltbay" by a local.
- **Mood colouring:** fear and anger select clipped variants and drop pleasantries; joy selects the expansive ones.

### 15.4 Facts into sentences (rumours, testimony, reports)

Every predicate registers a template in each tense, and confidence chooses a hedge:

```
pred killed     : "{subj} killed {obj}{at: at {place}}"
conf high, saw  : "I saw it with my own eyes: {fact}."
conf high, told : "{source} told me {fact}, and {source.he} doesn't lie."
conf mid        : "They say {fact}."
conf low        : "I heard a whisper, mind you only a whisper, that {fact}."
false, liar     : (the same templates: a liar sounds like anyone else)
```

So every fact anyone holds can be spoken in many ways by anyone, and rumours, testimony to a guard, a spy's report and a priest's denunciation are the same code. Grammar helpers handle a/an, plurals, he/she/they and capitals.

### 15.5 How big is the "wealth"

Authored: say 150 concepts (greetings, farewells, weather, work, needs, 40 reaction concepts, 30 topic concepts, faith, trade, refusals, threats) x about 6 lines per concept per register, for 8 registers: about 7 000 lines, a big but normal amount for an RPG (Morrowind has far more text) and easy to grow in files. Multiplied at runtime by slot fillers (address, oath, native words, places, names, weather), tics and fact sentences, the distinct output runs into the hundreds of thousands, and the right one for the moment is found by the criteria, not by luck.

Writing the bank: Claude can draft lines in bulk at authoring time (offline, in the text files, for you to cut and edit); the game never calls a model. Two tools keep it healthy:
- `dialogue --lint`: every slot resolves, every concept has a fallback line with no criteria, every line is 7-bit ASCII and no wider than the dialogue box.
- `dialogue --sample <seed>`: prints a few hundred lines from random people in random situations, so a bank can be read and judged in one sitting.

### 15.6 Topics (talking to the player)

- **The topic list** is: standard topics (work, the town, rumours, directions, the gods, little advice) + the places, people and factions the NPC holds facts about + topics unlocked by facts the player holds (ask about the dead sailor only once you know there is one).
- **Rumours:** the speaker's juiciest held fact that their trust in you allows, spoken through 15.4.
- **Directions:** built from the portal route through the place graph, naming landmark places along it: "The chapel? Past the well, left at the tannery, you'll smell it before you see it." A small template per turn type; the landmarks are places the speaker knows.
- **Busy people:** someone mid-job says so ("Come back when the bread's out.") and the estimate from tach-006 can even say when.
- **Checks:** persuade / intimidate / bribe / lie use skill + reputation axes + stance; success and failure have their own concepts, so a failed intimidation has lines too.

### 15.7 The Veil and delusions

The gothic RPG separates world and perception. Speech follows the same rule: when the player's Veil is thin, a line can be swapped **for display only** for a veil variant (concept `veil:<concept>`, deterministic per speaker and night, so it can be learned); the NPC really said the original and acts on it. Delusions that change NPC behaviour are false facts in that NPC's knowledge (the pall's gaslighting), so they speak and act on them truthfully, which is more unsettling than random nonsense.

### 15.8 A sample of the bank

To show the range (ASCII, short lines for a terminal box, braces are slots):

```
greet       plain     "Evening. Mind the step, it's rotten."
greet       coarse    "What d'you want? Spit it out."
greet       courtly   "Ah. {address}. One hears so little of you, and all of it odd."
greet       pious     "{oath}. Peace on your road, {address}."
greet       archaic   "The crest bows. Speak, small-wing, and be heard."
greet       florid    "A customer! Or a thief. Either way, welcome, welcome."
weather     plain     "Rain again. The reeds'll be under by Thursday."
weather     clipped   "Fog. Bad for the watch."
weather     archaic   "The wind turns its face. Old rains come from that side."
fear.fire   any       "Fire! Fire in the {place.kind}! Water, damn you, water!"
fear.corpse coarse    "Gods. That's Tomas. That's Tomas, that is."
fear.corpse pious     "{oath}... someone fetch the priest. And the watch."
busy        plain     "Can't stop, the dough won't wait. Back in {eta}."
need.hungry coarse    "My belly thinks my throat's been cut."
need.tired  courtly   "Forgive me, I have not slept since the bells."
work.smith  clipped   "Iron's hot. Talk fast."
rumour.mid  plain     "They say {fact}. I say nothing. Walls have ears."
rumour.low  florid    "Now I'd never repeat it, but, well, it's said {fact}."
refuse      coarse    "Ask someone who cares."
refuse      archaic   "That is not a thing told to the crestless."
threat      clipped   "Hand off the hilt or lose the hand."
threat      courtly   "I would think very carefully, were I you. And I am glad I am not."
intimidated plain     "All right! All right. Take it. Just go."
bribe.ok    florid    "Generous! The gods smile on generous people. Briefly."
faith.x     pious     "Be still, and the world is still with you."
faith.z     pious     "Want it. Want it more. That is the first prayer."
faith.w     pious     "Every debt is paid. Every one."
faith.y     clipped   "Stand by your word. That's all of it."
spy.report  clipped   "Two crates at the east quay. No marks. Night watch looked away."
spy.cover   florid    "Wool, sir, only wool, and the finest this side of the sea."
farewell    plain     "Go careful."
farewell    archaic   "May your shadow keep its shape."
```

## 16. Runtime cost

| Tier | Who | What runs | Cost (estimate) |
|---|---|---|---|
| Near | creatures in awake chunks (typically under 200) | `ai::act` as today, plus stimulus reads instead of the all-creatures scan | same as now or less once `look_around` uses the chunk index |
| Town | people with bodies in sleeping chunks | nothing per tick; on wake: `where_at`, needs by `dt`, jobs by `Span`; hourly gossip pass | gossip: about 1 us per person per game hour |
| Far | bodiless people, other regions | daily: agenda, storylets, ties drift, faith | a few us per person per game day |
| World | factions, networks, influence fields | daily | under 1 ms per game day for an island |
| Speech | only speakers in earshot of the player | line selection | microseconds per line |

Memory: about 1.5 KB per person, the ledger of facts (capped by eviction of facts nobody holds), the line bank (a few hundred KB of text). An island of 2 000 people fits in a few MB and costs well under a millisecond per game hour beyond what the near creatures cost today.

## 17. Engine and game

| Engine (`life/`) | Game (gothic RPG shown) |
|---|---|
| `People`, `Persona` vectors, `Mood`, households | the trait list; persona defaults per culture (conquistadors greedy and proud, bird people patient and archaic) |
| ties, factions, regard rules machinery | regard rules, the guilds, houses, the Templars |
| `Fact`, ledger, knowledge, gossip, reputation sums | predicates, juiciness, reputation axes, inference rules |
| stimulus bus, reaction table runner | stim kinds (loud spell, desecration), the reaction table |
| places, portal graph, Dijkstra maps, access | places authored in Tokamak, stations and timetables |
| routines with jobs, calendars, `where_at` | role templates, holy days, market days |
| aims and schemes runner, director budget | the aims, schemes, director budgets per region |
| creeds, powers, influence fields, pacts | the nine gods, counter pairs, creeds, cults, omens |
| line bank, matcher, slots, hedges, lint, sample | the lines, registers, oaths, native words, veil variants |

Broadside can use the same: crew morale is mood, mutiny is an aim with schemes, sailors' barks are lines, a ship is a faction. The desert and spy games are mostly reputation, facts and networks.

## 18. Changes to existing files

- `mind.rs`: `Mind` gains `person: Option<PersonId>`, `mood`, `job` (tach-006), `today: Vec<Duty>` (agenda additions); `Task` gains `Job`, `Visit`, `Attend`, `Rite`; `Goal` gains `Investigate(Pos)`, `Help`, `Talk(ThingId)`, `Use { place, spot }`. `Routine::where_at`.
- `ai.rs`: `urgent()` reads stimuli and the reaction table; `plan()` adds drives and shelter to the scored options; `look_around` and `find_food` use a chunk index; temperament read from the creature; flee uses a danger map.
- `needs.rs`: unchanged; drives are a separate small struct.
- `path.rs`: `search()` and `neighbours()` take an `Access`; long routes go through `nav.rs`.
- `mod.rs`: `Life` holds `People`, `Ledger`, factions, places; `step` turns events into stimuli; `Creature` gets its own `Temperament`; things get an optional owner.
- `species.rs`: temperament becomes the default for individuals; `walk` becomes metres per second (tach-006 step 3).
- `world.rs` (not `life/`): a `topo_version` per chunk, bumped when passability changes.

## 19. Build order, smallest first

Each step ships alone and keeps tests green. Steps 1 to 3 of tach-006 (day length, `Span`, speed in m/s) come first.

1. **Temperament per creature** with seeded jitter. Tiny. Test: two dogs differ; the same seed gives the same dogs.
2. **Chunk index for creatures and things; `look_around` and `find_food` use it.** Small. Test: same behaviour, bench at 500 creatures.
3. **Stimulus bus** from `LifeEvent`s, hearing through walls, and a minimal reaction table (shout, investigate, flee, help). Small to moderate. Test: a scream behind one wall is heard, behind four is not; a guard investigates.
4. **Places and access** (`Visit`, private doors, keys, trespass stimulus). Moderate. Test: a stranger routes round a private house; the owner goes through.
5. **`People`, persona, mood, ties, factions** (with households and ownership). Moderate. Test: eating from the household store, not the neighbour's.
6. **Routines: calendar, jitter, `where_at`, wake-time placement.** Moderate. Test: skip 6 hours, wake the chunk, the smith is at the anvil.
7. **Facts, knowledge, gossip, reputation sums.** Moderate. Test: a witnessed murder is known town-wide in about two game days, not in the next town; a false fact spreads like a true one.
8. **Dialogue: line bank format, matcher, slots, hedges, `--lint`, `--sample`.** Moderate; can start in parallel after step 5. Test: the most specific line wins; no repeats within 8 lines; all output ASCII.
9. **Portal graph and Dijkstra maps.** Moderate to large. Test: a 2 km route under a millisecond; a crowd leaving a burning hall gets out.
10. **Drives and activities.** Small once 5 exists.
11. **Aims, schemes, agenda tick, director.** Moderate.
12. **Creeds, powers, influence fields, pacts.** Moderate. Test: an all-z district drifts, the x faithful respond, the Allfather term restores it.
13. **Networks** (secret factions, reporting, turning, contradiction checks). Small once 5, 7 and 11 exist; it is mostly game data.
14. **Far tier** (bodiless people ticked daily, travel as jobs, stations). Moderate; with tach-006 step 11.

## 20. Open questions

1. **How many people?** An island of 500, 2 000 or 10 000 named people changes the far-tier budget and whether every person needs a full record (or whether far crowds are just counts per town until the player arrives).
2. **Persona axes:** are the twelve suggested traits right for you? Should the two god axes be persona traits (my suggestion) or kept separate?
3. **How much should the player see of minds?** A DF-style "thoughts" panel (why the smith is angry), or only what the NPC says and does?
4. **The director:** one global drama budget, per region, or none (let it burn)?
5. **The Allfather:** the restoring term makes "alive and distant" vs "dead" a playable difference. Do you want it to change during the game?
6. **Gossip speed:** how fast should news travel? Two game days across a town and a week to the next town is my default.
7. **Line authoring:** are you happy for Claude to draft line banks offline for you to edit, and which registers and cultures matter first?
8. **Directions and maps:** should NPCs who do not know a place say so, and should the player's own map knowledge come from asking (as in Morrowind's "follow the road north")?
9. **Prejudice as a mechanic:** stereotype regard between cultures is a modelled force here. How visible should it be, and should the player be able to move it (a town's view of a people changing over the game)?
10. **Children:** should births and ageing run in real game time (years), or should a game span weeks so children stay children? This decides whether aging needs a calendar year.
11. **Spy networks and the player:** can the player found their own network (recruit agents, run handlers), or only serve existing ones?
12. **Which comes first after tach-006:** stimuli and reactions (towns feel alive soonest), or people and facts (reputation and spies soonest)?
