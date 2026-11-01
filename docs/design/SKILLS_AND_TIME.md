# Skills, task time and the game clock (design proposal, tach-006)

Status: proposal only. No code has changed. Written against the engine as of 1 Nov 2026 (`src/grid/clock.rs`, `src/life/*`), plus Broadside's `crew.rs` and `drill.rs` read for reference.

Your brief (29 Oct): creatures have a proficiency in skills and it changes how long a task takes; one standard way to work out how long anything takes; the world should feel real (long walks are long, a big meal takes ages); manage time as a fraction of the day so the day length is a setting.

## 1. The short version

1. **One unit of time for gameplay: the fraction of a day.** A `Span` is "0.25 of a day" (6 hours). Authors write `Span::hours(2.0)` or `Span::minutes(30.0)`; both are just fractions of a day underneath. The clock has a `day_secs` setting (default 86400 game seconds). Change it and every chore, every hunger rate and the sun move with it.
2. **One formula for how long anything takes:**
   `time = base x skill x body x load x place x help` (details in section 4). Same for the player and for NPCs. Nothing in the engine is "instant" except things that really are (a reflex, a spark).
3. **A task is a `Job` with progress 0..1**, not a stopwatch. Each tick it advances by `1 / (current duration in ticks)`. If you get tired or hurt halfway, the remaining time changes. If something interrupts it, the progress is kept or lost depending on the kind of job.
4. **Skills are a plain table:** the game registers its skill names once, each creature has a `Vec<f32>` of proficiencies 0..1 indexed by skill id. Broadside's 15 crew skills are a table of 15; the gothic RPG's "skills for everything" is a table of 80. The engine does not know what any skill means.
5. **Long things fast-forward.** A 30 km walk is about 6 game hours. The game lets the clock run fast while the player is busy (the sandbox already does this when you sleep) and stops it when something happens.

## 2. What the code does today (so we know what changes)

- `Clock` holds `tick`, `secs_per_tick` and `start_secs`. `day()` and `hour()` divide by a hard-coded `86400.0`. The same number is hard-coded in `needs.rs` (`DAY`), `body.rs` (wound healing, bruises) and `life/mod.rs` (stain drying). So the day length cannot be changed today.
- `Life::step` already works in game seconds (`dt` = seconds since last step), not ticks. So skipping time (`Clock::skip`) already ages hunger and wounds correctly. This is a good base to build fast-forward on.
- Walking speed is `Species::walk` = **ticks per step**. At the sandbox's 2 s per tick a person needs 2 ticks = 4 s per metre (0.25 m/s, about 5x slower than a real walk), and changing `secs_per_tick` silently changes how fast everyone walks. Broadside uses 0.5 s per tick. Also, a creature moves at most one cell per tick, so a tick longer than about 0.7 s caps walking below real speed.
- `Task` in `mind.rs` is a routine entry ("be at the smithy 8 to 17"), not a job with a length. `Goal::Eat`, `Drink`, `Sleep` take effect at once (`Life::eat` removes the food and cuts hunger in one call).
- `Capacities` (conscious, moving, grasp, sight, ...) already describes what a hurt body can do, and the AI already divides step time by `moving`. That is the "condition" idea in miniature.
- Broadside already has the pattern we want, in `drill.rs::step_time`: `base_s x (0.6 + (1 - skill))`, then multipliers for load, fatigue, physique, doing someone else's job, floor of 0.8 s. And `crew.rs` stores `skills: [f32; 15]`, each 0.01 to 0.98. The proposal below is that function, generalised and moved into the engine.

## 3. Time: one unit, and what scales with it

### 3.1 The types (plain-language Rust)

- `Span` is a **newtype**: `struct Span(f64)`. In Python terms, a float with a name, so the compiler stops you adding "seconds" to "fractions of a day" by accident. It holds a fraction of a day. Constructors: `Span::days(x)`, `Span::hours(x)` (x / 24), `Span::minutes(x)` (x / 1440).
- `Clock` gets one new field, `day_secs: f64` (default 86400.0), and a few small methods:
  - `day_fraction()` gives 0..1 through today (`hour()` becomes `day_fraction() * 24`, so routines written in hours still work and still mean "24 slices of the day").
  - `ticks_in(span) -> u64` turns a `Span` into whole ticks: `span x day_secs / secs_per_tick`, rounded, at least 1.
  - `span_of_secs(secs) -> Span` goes the other way (for physics that gives seconds).
- **Deadlines are whole ticks (`u64`), authoring is fractions.** A job stores `finish_tick` computed once with `ticks_in`. Whole numbers do not drift, so saves and replays stay exact. (`mind.rs` already stores `next_act: u64`, so this is the same habit.)

### 3.2 Two kinds of time

The one real design decision. Some things should follow the day length, some should not.

| Follows the day (`Span`, fractions of a day) | Stays in physical seconds (per tick, as now) |
|---|---|
| chores and crafts (cook, build, mend, forge) | fluid flow, heat spread, gas, fire spread |
| travel between places (distance / speed) | falling, projectiles in flight, explosions |
| hunger, thirst, tiredness rates | bleeding, clotting seconds, drowning seconds |
| wound and bruise healing, sleep length | combat swing timing, a single step in a chase |
| sun, temperature curve, routines, shop hours | rain, wind gusts |

Why: if a game makes the day shorter (say a 2 hour day so a session sees several), cooking a stew should still take "a tenth of a day", and you still starve in about a month of days. But a fire should not burn 12 times faster, and a fall should not hurt differently. So **gameplay durations are `Span`s; physics keeps `secs_per_tick`.** Both read the same clock, so nothing disagrees.

Recommended defaults: `day_secs = 86400` (24 real-world hours, so "6 hours" is believable), `secs_per_tick` per game as now. The wall-clock rate (ticks per second) is the *player's* speed setting and is separate again (the sandbox's `SPEEDS`).

### 3.3 Believable numbers at the standard 86400 s day

Walking speed 1.4 m/s (5 km/h). One day fraction = 24 hours.

| What | Real feel | As a `Span` |
|---|---|---|
| walk 1 km, flat | 12 min | 0.0083 |
| walk 10 km | 2 h | 0.083 |
| walk 30 km (a hard day's leg is 8 h) | 6 h | 0.25 |
| across a 5 km island | 1 h | 0.042 |
| eat a snack (nutrition 0.1) | 5 min | 0.0035 |
| eat an ordinary meal (0.3) | 20 min | 0.014 |
| eat a feast (0.6, sitting at table) | 1.5 h | 0.06 |
| drink from a stream | 1 min | 0.0007 |
| porridge for 4 (10 min prep + 20 simmer) | 30 min | 0.021 |
| stew for 4 (30 min prep + 2 h simmer) | 2.5 h | 0.10 |
| roast feast for 30 (see 6.2) | about 5 h | 0.21 |
| sleep to clear tiredness 0.8 (the engine takes off 3.3 a day asleep) | about 6 h | 0.24 |
| fell a tree with an axe | 20 min | 0.014 |
| a sailor loads and fires one gun (Broadside drill, sum of steps) | about 35 s | 0.0004 |

The last row is the useful one: Broadside's gun drill is *seconds*, the same formula as a 5 hour feast. That is why one formula with one unit is possible. A `Span` that small is fine, it is just a very small fraction (`f64` has plenty of precision).

## 4. The standard duration formula

Every action that takes time declares a small **task spec** (data, not code):

```
TaskSpec {
    name:     "chop wood",
    base:     Span::minutes(20.0),   // a person of average skill, rested, unhurt, bare conditions
    skill:    Some(SKILL_WOODCUTTING),
    spread:   0.5,                   // how much skill matters (0 = not at all)
    needs:    Does::GRASP,           // body capacity required (hands, legs ...)
    effort:   0.8,                   // 0 deft .. 1 heavy: how much tiredness and load matter
    ...
}
```

and the engine works out the duration with one function:

`duration = base x skill_mult x body_mult x load_mult x place_mult x help_mult`

then clamps the product to 0.25x .. 8x of `base` (so one bad day cannot make a 20 minute job take a week), with a floor of 1 tick.

**skill_mult** (from the creature's proficiency `p` in 0..1). Broadside's `0.6 + (1 - p)` is a range of 1.6x (untrained) down to 0.6x (master), a 2.7x spread. Generalised: `skill_mult = 1 + spread x (0.6 - p)`. With `spread = 1.0` that is exactly Broadside's. A spec with a bigger spread (lock picking, surgery) makes mastery matter more; `spread = 0` ignores skill (walking). If a task has no skill, `skill_mult = 1`.

**body_mult** (the creature's own state):
- fatigue: `1 + 0.6 x fatigue x (0.3 + effort)` (copied from `drill.rs`).
- capacity: divide by the relevant `Capacities` entry (`moving` for travel, `grasp` for manual work, `sight` for fine work). Already how the AI treats `moving`.
- pain and needs: use the existing `Needs::factor()` (hungry, parched, frozen slow you) and `caps.pain`.
- physique/strength: Broadside's `physique` as `t / physique^(0.3 + effort)`. Stays a game-supplied number at first (open question 5).

**load_mult** (what is being moved or lifted): `(mass / reference_mass)^k`, as `drill.rs` does for shot and guns. Carrying a pack slows travel the same way.

**place_mult** (the world around it, read from the grid at the doer's cell):
- light level for fine work (candle vs noon),
- wet, cold or hot air (`world.temp`, `needs.wet`),
- footing for travel: terrain from the path costs (road 0.8x, grass 1x, mud 1.4x, swimming 3x; the pathfinder already prices swimming at 3x),
- tool quality (a sharp axe 0.8x, a blunt one 1.3x) from the held thing.

**help_mult**: extra hands on a *shared* task: `n^-0.8` for n equally good workers (two people are about 1.7x as fast, ten about 6x). Gun crews in Broadside are the other model, separate stations each doing their own step; both must stay possible (section 5.3).

Properties we want, and this formula has:
- **Same function for everyone.** The player's "chop wood" and a woodcutter NPC's call the same `duration`. The only difference is who is deciding to start.
- **Pure.** It takes the spec, the doer's numbers and a few readings from the world, and returns a `Span`. No world mutation, so it is easy to unit-test (and the first thing to test: a master is faster than a novice, a tired man is slower, a longer distance takes longer).
- **Shown to the player before committing:** "Cook stew: about 2 h 40 min". NPC planning uses the same call (section 7).
- **Tunable in data.** Making a game feel slower or faster is editing `base` and `spread`, or the `day_secs` setting, not code.

## 5. Jobs: how a task actually runs

### 5.1 Job state

A creature gets one `job: Option<Job>` (inside its `Mind`, next to `goal`). I call it `Job` because `Task` is already taken by routine entries in `mind.rs`.

```
Job { spec: TaskSpec id, target: Option<ThingId or Pos>, progress: f32 (0..1),
      phase: usize, started: tick }
```

Each tick (inside `ai::act`, only when the creature is working), `progress += 1.0 / duration_ticks_now`. Duration is recomputed every so often (say every minute of game time, not every tick) from the formula, so tiredness, a new wound or dusk falling gradually change the finish time. This is why it is a progress bar and not a fixed `finish_tick`. Estimates for the UI/planning use the same formula.

At `progress >= 1.0` the effect happens (food eaten, plank made) and a `LifeEvent::JobDone` is pushed, like the other events today.

### 5.2 Interruption

Each spec says what a break does: `keep` (building, mending: progress stays), `lose` (frying: spoiled), `rewind` (loses a share). Danger (the existing `alarmed` flag), a need becoming critical, or the player cancelling interrupts. A `LifeEvent::JobInterrupted` is pushed so the game can say why.

### 5.3 Attended and unattended time, and phases

Cooking shows why a job can have **phases**. Chopping and stirring need the cook (skill matters, hands busy); simmering needs only a fire (skill hardly matters, the cook may walk away). A spec is a short list of phases, each with `base`, a flag `attended`, and whether `skill` applies. Unattended phases run on the *thing* (the pot) or a timer, not on the creature, and finish even if the cook has gone to bed (and burn if left too long, a later nicety).

Broadside's drill maps on directly: each of the 9 drill steps is a phase with its own `station` (number in the crew), `skill` and `effort`; the "not your station" 1.2x penalty is a `help_mult` entry. So `drill.rs` later calls the engine's `duration` instead of its own `step_time`; no game rules are lost.

## 6. Travel and cooking in detail

### 6.1 Travel

A journey is a job of kind "go to": its duration is `path length x cost per cell / speed`.

- **Speed** is a physical number on the species, **metres per second** (human 1.4, running about 3 to 5, sheep, dog ...). This replaces `walk` (ticks per step), which is tied to the tick length today. The AI turns speed into "ticks per cell" with the clock, so changing `secs_per_tick` no longer changes how fast people are. (If a cell takes less than one tick the creature can only manage one cell per tick, so games need `secs_per_tick` at about 0.7 or less for real walking, or allow a creature several cells per tick later. See open question 2.)
- **Terrain multipliers** come from the cost the pathfinder already gives each step (10 straight, 14 diagonal, x3 swimming). A road or beaten path is cheaper; mud, snow, scrub and steep ground dearer. A hill (climbing a z level) costs extra time like stairs do now.
- **Conditions**: fatigue, load, `moving` capacity, rain and cold, and a possible skill ("wayfinding"/"seamanship") that cuts the time wasted on a trail.
- **Pace**: walk, hurry, run: three `effort` levels. Running is 2x speed (the AI already does this) but tires faster.
- **Long trips** show the cost honestly: the player picks a destination, the game shows "about 6 h 10 min, you will arrive after dark", and on go the clock fast-forwards while the player walks (as sleeping already speeds the sandbox up), slowing down or stopping on an event (a creature appears, rain starts, hunger crosses a line).
- **Same for ships.** Broadside's passage time falls out of the same speed/distance/conditions (wind, current come from the sea fields), not from a separate rule.

Believable pacing check: a village with the nearest town 12 km away is 2 h 20 min each way: a morning errand, not "just walk there". A kingdom 400 km across is 3 to 4 weeks on foot. That is what you asked for.

### 6.2 Cooking and big meals

Time scales with how much you make, but not in a straight line (cutting for 30 is not 7.5x the work of cutting for 4):

`prep = base_prep x (servings / base_servings)^0.6`, divided by `help_mult` for extra cooks.
`cook (unattended) = fixed by the dish` (a roast takes the time a roast takes, however many cooks).

Roast feast for 30, one cook of average skill: prep 30 min x (7.5)^0.6 = about 100 min; roasting 3 h unattended. A master cook (0.6x on the prep only) finishes prep in 60 min; two cooks about 57 min. Wall-clock total for the first cook is just under 5 h, which matches the table.

Eating takes time too: `eat_time = nutrition x 40 min x (1 - 0.3 x table_seating)`; a hungry man eats faster. Eating becomes a job, so an NPC "having dinner" is visibly occupied for 20 minutes, can be interrupted by a fight, and the food is consumed bite by bite (progress).

The cook-time of an unattended phase can later be tied to the heat sim (a pot only cooks above about 90 C at its cell, so a weak fire takes longer) with no change to the design: it is another multiplier from `place_mult`.

## 7. Skills

### 7.1 Generic attach

```
SkillId(u16)                      // newtype, an index
Life.skills: Vec<SkillDef>        // registered by the game, like species and materials
SkillDef { name, spread, learn_rate, ... }
Creature.skills: Vec<f32>         // one proficiency 0..1 per registered skill
```

- **Registered by the game, not the engine.** The same way `Life::add_species` works. Broadside registers its 15 names; the gothic RPG registers its list; the engine ships none (or a tiny optional starter set, like `Materials::standard()`).
- **A plain vector indexed by id** is fastest and simplest: 80 skills x 4 bytes x 1000 creatures = 320 KB. In Python terms it is `skills[SKILL_COOKING]`. A `SkillId` newtype means you cannot pass a species id by mistake.
- **0..1 float**, because Broadside already uses it (clamped 0.02 to 0.98). A game that likes levels 0 to 100 divides by 100 at its edge. The scale means: 0 untrained, 0.3 apprentice, 0.6 skilled, 0.9 master.
- **A species can limit what is possible:** the task's `needs: Does::GRASP` checks the body (a dog cannot chop wood; a one-armed man can, slowly). That is the existing `Does` bit flags, no new idea.
- **Background and starting skills are the game's job** (Broadside's `Background` table). The engine just offers `creature.skills[id] = value` and `skill(id)`.

### 7.2 Learning

Use improves skill: when a job finishes, `skill += learn_rate x difficulty_gap x duration_in_days`, where the gap shrinks to zero as the task gets easy for you (a master gains nothing from porridge). Slows towards 1.0. Optional per-skill decay when unused for months, and teachers (a teacher present multiplies the gain) later. Gains are data (`learn_rate` per skill), so a game can make skills sticky or fast.

### 7.3 Skill beyond time

The same proficiency should also drive quality of the result and the chance of failure (a poor cook burns the stew). That is out of scope here; the point is that **one number per skill** feeds all three, so adding quality later needs no new data on creatures.

## 8. How NPCs use it (links to the AI/NPC requirement)

- A routine duty ("smith: forge 8 to 17") becomes a *loop of jobs* chosen from what is at the forge. The routine says where and when; the job system says how long each piece takes.
- Planning back from a deadline works for free: "dinner at 18:00, the stew takes 2.5 h, start by 15:00 minus the walk to the kitchen". NPCs call `estimate()`, the player's UI calls it too.
- **Cheap off-screen simulation:** a job is "doing X, progress p, finishes about tick T". NPCs outside the awake chunks (or a far village) do not need per-tick work; when the player returns, advance their jobs by the elapsed `Span` in one step (their needs already update by `dt`). A world with long travel needs this, because the player will be away for days.
- Dialogue, ambition and religion plug into this later through what an NPC is *doing and available for* (an NPC mid-job is not free to talk; a busy cook says "come back later"). They are not part of this design.

## 9. Engine changes, smallest first

Each can ship on its own and keep the tests green. "Owner" is from `CLAUDE.md`: message the "Tachyon engine setup" session before touching `life/`; `grid/clock.rs` has no named owner.

1. **`Clock::day_secs`.** Add the field (default 86400), `day_fraction()`, `ticks_in(Span)`, `span_of_secs`; make `day()`/`hour()`/`daylight()`/`ambient_temp()` use it. Replace the four hard-coded `86400.0`s (`needs.rs`, `body.rs` x2, `life/mod.rs`) with the day length, passed in as a number. Test: set `day_secs` to half, run the same number of ticks, hunger doubles per tick. Tiny.
2. **`Span` newtype** in `base/` or `grid/clock.rs`, with `days/hours/minutes` and tests. Tiny.
3. **Species speed in metres per second.** Replace `walk` (ticks per step) with `speed`; `ai.rs` converts with the clock when setting `next_act`. Keep a default that gives today's feel so existing tests pass. Test: halving `secs_per_tick` keeps the time to walk 100 m unchanged. Small, but touches all species and the AI timing, so coordinate.
4. **`life/skills.rs`:** `SkillId`, `SkillDef`, `Life::add_skill`, `Creature.skills: Vec<f32>` (grown when a creature is added). No behaviour yet. Small.
5. **`life/task.rs`: `TaskSpec` and the pure `duration()` function** with the formula of section 4 and unit tests (master faster than novice; tired slower; heavier takes longer; clamp holds; `spread = 0` ignores skill). Moderate; no world access, so it can be done in isolation.
6. **`Job` on `Mind` and the runner** in `ai::act`: progress per tick, recomputed duration, `JobDone`/`JobInterrupted` events, keep/lose rules. Moderate.
7. **Eating, drinking and sleeping become jobs.** `Life::eat` and `drink` stay as the "effect on completion"; `Goal::Eat`/`Drink`/`Sleep` start a job first. Test: a big meal takes longer than a snack, fed in the same tick count as before for a trivially small meal. Moderate; changes behaviour that existing life tests check, so update them.
8. **Estimates and journeys:** `estimate(job)` for the UI/planner; a travel job built from `path::find` with terrain multipliers; `Scene::fast_forward(until)` that advances ticks without drawing and stops on events (the sandbox's sleep speed-up generalised). Moderate.
9. **Phases and attended time** (cooking, Broadside's drill steps). Moderate.
10. **Learning** (skill gain on completion, optional decay). Small once 6 exists.
11. **Off-screen NPC catch-up:** advance jobs and routines by an elapsed `Span` for creatures in sleeping chunks. Moderate-large; do last, needs a decision about how far the world simulates (open question 8).
12. **Broadside migration:** `crew.rs` registers its 15 skills with the engine; `drill::step_time` becomes a call to `duration()`. Done in the Broadside repo by its owner.

## 10. Open questions (decide before step 3 and step 5)

1. **Does a shorter day speed up walking?** My recommendation: yes for travel between places (a journey of "a third of a day" stays that), no for physics and combat swings. But this means walking speed in metres per game-second varies with `day_secs`. If you would rather speed stay fixed in metres per second and only chores scale, say so; it is one switch in step 3.
2. **Canonical `secs_per_tick`?** Sandbox is 2 s, Broadside 0.5 s. Real walking needs about 0.7 s or less per cell. Do we set 1 s as the engine default, or let games choose and allow several cells per tick?
3. **How strong should mastery be?** Broadside's range is 1.6x to 0.6x (2.7x). Good for a gun crew; probably too little for a surgeon or smith. Plan: per-skill `spread`, default 1.0. Right?
4. **Can the untrained attempt anything?** Option A: always allowed, just slow. Option B: each spec has `min_skill` below which the player gets "you don't know how". I lean to A with a few B tasks (surgery, spellcasting, navigation).
5. **Attributes.** Broadside has `physique`, `bravery`, `discipline` next to skills. Should the engine have a generic `Attributes` vector in the same style (strength, wits ...), with specs naming which attribute applies? I would add it with the same pattern after step 5, not before.
6. **Teams.** For shared tasks, is `n^-0.8` enough, or do we need stations like the gun drill as the general case? (Drill shows stations are right when the roles differ.)
7. **Waiting for the player.** When a job is long, does the player watch the clock run at fast speed (NPCs move, interruptions happen), or choose "wait/skip until done"? The first is safer and already works for sleeping; the second needs the off-screen catch-up of step 11.
8. **How far does the world run while the player is away?** Only awake chunks simulate today. For a week-long trip, do distant NPCs get the cheap catch-up in step 11 (their jobs and routines advance by the elapsed time), or do we freeze them?
9. **Calendar.** Only `day()` exists. Seasons, a year length and moon phases would be fractions of a longer cycle in the same style (`year_days` as a setting). Needed soon (farming, snow) or later?
10. **Cooking and the heat sim.** Is it enough that an unattended phase has a fixed time, or should a weak fire make it genuinely slower by reading the cell temperature? (Easy to add, only worth it if you want a cold hearth to matter.)
11. **Name clash.** `Task` already means a routine entry in `mind.rs`. Calling the new thing `Job` is my suggestion; rename if you prefer.
12. **Skill scale for the gothic RPG.** Is 0..1 fine, or do you want 0 to 100 shown in-game with the engine keeping 0..1 underneath (my suggestion)?
