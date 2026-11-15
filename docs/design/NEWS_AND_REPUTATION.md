# News, reputation and collective belief (design note, tach-025)

Status: proposal only, no code. Your answers it builds on:
- **14 Nov:** news travels by population density, like energy transfer. Detail dissipates with distance, and minor details may be invented or exaggerated within reason. The same model serves reputation. Beliefs converge within a settlement and diverge and strengthen between opposing factions.
- **NPC_MINDS answers 6 and 9:** an energy-like system sensitive to population density. Views coalesce like heat.
- **SIM_LOD decision:** far events reach the player as news or rumour.

It sits on top of:
- `design/NPC_MINDS.md`: `Fact`, `Held`, gossip (9.2), reputation (10), factions (9.1), and turning facts into sentences (15.4).
- `SIM_LOD.md`: the L0 to L3 ladder.
- `ROADMAP.md` 3.6: the region layer.

Every number below is a starting value to tune, not a measurement.

## 1. The short version

- **A story is a fact plus spreading state.** It has a newsworthiness `W`, the slots it can be told by (who, what, where, when, how many), and an origin node.
- **The medium is people.** Stories move over a **network** of settlements (nodes), linked by routes (edges). Each edge carries three numbers worked out from the population density along it: travel time `tau`, loss `A` and retellings `N`. Dense country carries a story fast and far, but tells it more often on the way.
- **Heat `h` is how much a place talks about a story.** It falls by `e^-A` along each edge and halves with time. When it drops below 1, the story stops spreading: its reach ends.
- **Detail `d` is how true the telling is.** Each retelling multiplies it by `1 - delta`. Low detail does two things: specifics blur up a ladder ("Brother Aldric" becomes "a Templar" becomes "soldiers"), and minor slots drift within set caps (numbers grow, the day slips). The core (what happened) never changes.
- **Reputation is what stories leave behind.** A story arriving at a node adds to the reputation of whoever the blurred "who" still names there. A deed blurred to "a Templar" stains the Templars, and one blurred to "someone" stains nobody.
- **Belief is a value per issue for each faction group in a node.** Groups on the same side pull together, like heat in a room. Opposed groups push apart and grow more certain. Stories nudge each group, and how certain a group already is decides whether it reads the story as evidence or as confirmation.
- **The player hears news or rumour.** It is **news** when it comes by an official channel, or when the detail is at least 0.7. Otherwise it is **rumour**. Either way it reaches the player through NPCs (topics, barks), a herald or a letter, never as an omniscient message.

## 2. Quantities

| Name | What | Unit / range | Starting value |
|---|---|---|---|
| `rho` | population density of the land a route crosses | people per km^2 | from region counts |
| `P` | population of a node | people | from region counts |
| `v(rho)` | carrier speed: `v_min + (v_max - v_min) rho / (rho + rho_half)`, x1.5 on a road | km per 24 h of game time | `v_min` 5, `v_max` 40, `rho_half` 20 |
| `tau` | edge travel time, `integral ds / v` | game days | |
| `l_h(rho)` | attenuation length, `l_0 (1 + rho / rho_half)` | km | `l_0` 15 |
| `A` | edge loss ("optical depth"), `integral ds / l_h` | - | |
| `n(rho)` | retellings per km, `n_0 + rho / rho_t` | per km | `n_0` 0.05, `rho_t` 400 |
| `N_node(P)` | retellings inside a node before a story leaves, `log2(1 + P / 200)` | - | |
| `W` | newsworthiness: `W_pred x (1 + log10(1 + magnitude)) x (1 + fame)` | 1 to 1000 | brawl 2, theft 3, murder 10, houses burned 8, battle 40, death of a ruler 300 |
| `h` | heat: starts at `W` at the origin | >= 0 | heard while `h >= 1` |
| `t_half(W)` | how long a place keeps talking, `2 (1 + log10 W)` | game days | |
| `f(h)` | share of a node that has heard, `1 - e^(-h/5)` | 0 to 1 | |
| `d` | detail, `(1 - delta)^N` along the path | 0 to 1 | `delta` 0.08 |
| `sal` | salience of a slot (how firmly it sticks) | 0 to 1 | what 1, where 0.8, who 0.6, when 0.5, how many 0.3 |
| `mu` | drift chance per retelling, scaled by `1 - sal` | - | 0.1 |
| `R[n,g,S,a]` | reputation of subject `S` on axis `a`, held by group `g` in node `n` | -2 to 2 (fixed point) | half-life 60 days |
| `x[n,g,k]`, `c[n,g,k]` | opinion on issue `k` (-1 to 1) and certainty (0 to 1) | | `c_0` 0.3 |
| `r[g,h]` | regard between groups (from faction regard, NPC_MINDS 9.1) | -1 to 1 | |

Units follow the time contract:
- **Transport is travel.** Speeds are per 24 h of game seconds (40 km/day is about 0.46 m/s averaged over walking, resting and waiting for a carrier) and are converted through the clock, like walking speed in tach-016.
- **Talk and opinion are social time.** Half-lives and daily rates are `Span`s in fractions of a day, like needs and routines.

## 3. How a story travels

**Network.** One node per settlement: towns, villages, hamlets, farmsteads above a small population, ports, monasteries and garrisons. Add waypoint nodes about every 10 km along a road through empty land, so a roadside inn hears news. Edges join neighbouring nodes. An edge's `tau`, `A` and `N` are path integrals over the region layer's density grid along the cheapest route (roads and rivers count). They are computed once and rebuilt weekly, or when the route changes (a bridge falls, bandits close a road). Sea lanes are edges between ports: `tau` is the voyage plus the mean wait for a sailing, `l_h` is 150 km, and `N` is 1 (the crew). The network can be authored in Tokamak (it is the place graph of NPC_MINDS 7.1, one level up) or derived from the atlas.

**Channels** are edge parameter sets over the same network:

| Channel | Carries | Speed | Loss | Retellings |
|---|---|---|---|---|
| gossip (default) | anything | `v(rho)` | `l_h(rho)` | `n(rho)` |
| sea lane | anything sailors hear | voyage + wait | 150 km | 1 |
| courier / letter | what a faction or household chooses to send | 60 km/day by road | none on the way; `h` = `W/2` at the recipient | 0 |
| herald / notice | proclamations | rides with a courier, then posted | none | 0 |

**Spread** runs once, when the story opens. It is a Dijkstra over arrival time from the origin. Each edge `i -> j` gives:

```
t_j = t_i + tau
h_j = h_i x e^(-A) x 2^(-tau / t_half)       (loss with distance, and staleness on the way)
N_j = N_i + N_node(P_i) + N_edge
d_j = (1 - delta)^N_j
```

A node whose `h_j < 1` is not entered, so the search stops by itself. The reach of a story is roughly `l_h x ln W`: a brawl dies in the next village, a battle crosses a province. The result is a list of `Arrival { node, t, h, d, variant }` sorted by `t`, queued for release. First passage wins: if a slower route arrives later, it is ignored in this version (see 9).

Why not plain heat diffusion on the grid? Diffusion leaks a trace of every story everywhere at once. Its front slows as the square root of time. And at 256 m cells it needs hundreds of steps per game day. The arrival-time form is the "ray" limit of the same physics: fronts move at the carrier speed, there is one search per story, and the reach is finite and has a meaning.

**After arrival**, a node's heat decays: `h(t) = h_arr x 2^(-(t - T) / t_half)`. The node holds the story while `h >= 1`, keeping at most 16 stories and dropping the coolest first.

## 4. Blur and drift (detail lost, details invented)

The story keeps the truth. An arrival keeps only how its telling differs from the truth.

**Blur: specifics go vague, never wrong.** Each slot type has a ladder (game data, words in the line bank), and `d` picks the rung:

| Slot | d >= rung 0 | rung 1 | rung 2 | below |
|---|---|---|---|---|
| who | 0.6 name ("Brother Aldric's Templars") | 0.35 faction or role ("a Templar patrol") | 0.15 kind ("soldiers") | "someone" |
| how many | 0.7 exact | 0.4 rounded (nearest 5 under 50, 10 under 200) | 0.15 word ("dozens") | "some" |
| where | 0.5 place | 0.25 district or region ("down on the coast") | 0.1 compass ("in the south") | "somewhere" |
| when | 0.6 day ("last night") | 0.3 rough ("two days back") | - | "lately" |

**Drift: minor slots change, within reason.** At each retelling, slot `s` drifts with probability `mu x (1 - sal_s)`. The draws come from a hash of (story, node, retelling), so they are deterministic per seed. A variant holds up to 4 drifts, and each drift follows a plausibility rule from the slot type:
- **how many:** x(1 + U[0, 0.3]) with chance 0.75, else x(1 - U[0, 0.15]). News grows in the telling. The total is capped at x3 and /1.5 against the truth.
- **who:** only while the rung is still a name. The name may swap to another person of the same role and faction within two edges of the origin (a plausible culprit), never to an outsider.
- **where:** may swap to a node one edge from the origin.
- **when:** +-1 day.
- **what:** never drifts, except along an escalation the predicate declares (wounded to killed, theft to robbery), at a quarter of the rate. Nothing is invented from nothing.

The ladders and caps are game data. The engine only applies them.

## 5. Reputation: what stories leave behind

Reputation splits into two parts:
- **Known-for (this section):** factual axes such as violence, piety and wealth, the same for everyone who heard.
- **Judgement (section 6):** whether that is good, which differs by group.

When an arrival is released at node `n`, for every group `g` there:

```
R[n,g,S,a] += v_a(pred) x f(h) x attach
attach = 1 for the person and 0.5 for their faction (who at rung 0)
       = 0.5 for the faction only                     (who at rung 1)
       = 0                                            (rung 2 or below)
```

What this gives:
- Far away, a deed blurs into a stain on the faction.
- Further still, nobody is blamed.
- A drifted "who" really can blame a namesake. That is a false belief, held truthfully.

Reputation decays with a 60-day half-life, so the player cannot remake themselves overnight, and it does not last for ever. Fame is `max_a |R|` at the origin node, and it raises the `W` of the subject's next story: famous people's deeds travel further. That is the NPC_MINDS 10 "deeds" term, now computed per node and per group instead of per community.

**Moving faster than the news.** The player can outrun news of their own deeds by taxi, or by mark and recall. A town hears what you did only when the story gets there.

## 6. Collective thought: converge within, diverge between

**Issues** are game-registered questions with a value from -1 to 1:
- "the Templars are just"
- "the old creed is heresy"
- "the player can be trusted"
- the god-axis leanings of NPC_MINDS 13

Regard for a faction or a person is an issue too, so faction regard and the judgement half of reputation are the same field.

**Within a node, once a game day,** for each group `g` with population share `p`:

```
pull = sum over h with r[g,h] > 0 of  p_h r[g,h] (x_h - x_g)          (friends: converge, like heat)
push = sum over h with r[g,h] < 0 of  p_h |r[g,h]| (x_h - x_g)        (opponents: move away)
x_g += (1 - c_g) (alpha pull - beta push)              clamp to [-1, 1];  alpha 0.3, beta 0.15
O_g  = sum over opponents of p_h |r[g,h]| |x_h - x_g| / 2              (exposure to disagreement)
c_g += gamma O_g (1 - c_g) - lambda (c_g - c_0)                        gamma 0.2, lambda 0.05
```

Certainty works as inertia: certain groups move less. Certainty rises when you live beside people who disagree, and relaxes to `c_0` in a town of one mind.

**A story's push** on an issue, with charge `q` (game data per predicate and issue; "Templars burned houses" carries -0.4 on "the Templars are just"), applies on release:

```
x_g += f d [ q (1 - c_g) + eta c_g |q| sign(x_g) ]          eta 0.5
```

The first term is evidence and moves everyone the same way. The second is confirmation: a certain group takes any story about the issue as support for what it already believes.

**Between nodes, also daily:**
- The same faction's groups in neighbouring nodes conduct: `x += kappa (x_other - x) / tau`, with `kappa` 0.05. A valley comes to one mind over weeks.
- Opposed factions across an edge repel by the same form with `beta`, so frontiers polarise.

**Individuals (the L0 bubble)** use the same three equations with ties in place of groups. The weights come from `Tie` regard, trust and familiarity. The update runs on NPC_MINDS's hourly gossip pass, and persona sets each person's `c_0` and the issues they care about. When the bubble demotes, each group's `x` and `c` are its members' means. When it promotes, members start at the group value plus a seeded jitter from persona.

**Boiling over** (open question 5): a node where opposed groups' `|x_g - x_h| > 1.2` and both `c > 0.6` emits an unrest event for the far simulation, at the cost of the region's director budget (NPC_MINDS 11).

## 7. How it plugs into the engine

| Where | What runs | Cadence |
|---|---|---|
| bubble, `Scene::tick` | nothing new per tick. Stimuli become facts as in NPC_MINDS; the stimulus bucket per chunk is the only chunk-scale link | - |
| bubble, gossip pass (N7) | facts witnessed or held by 3+ people with `W >= 1.5` open a story at the node of `region_of(pos)` (`pos / REGION`) | game hour |
| region layer L2 / L3 (SIM_LOD) | far events (flood, fire, plague, battle, a death, a migration) open stories directly from the event queue | as they happen |
| `news.spread(story)` | Dijkstra on the network, sliced to a budget per tick if it ever needs to be | once per story |
| `news.release(now, focus_nodes)` | pop due arrivals for the player's node and its neighbours: heard list, reputation, story push | game hour |
| `news.daily(now)` | release everything due, decay heat, evict, opinion update, reputation decay, conduction between nodes | game day (L3 tick) |

**Bubble to network.** When a story opens inside the bubble, its origin heat is `W`. While the bubble holds the player's node, the node's heard list mirrors the bubble's people. On demotion, `h = -5 ln(1 - share who know)` (the inverse of `f`), `d` is their mean confidence, and the variant is the most-held one.

**Network to bubble.** When an arrival is released at the player's node:
- `k = max(1, round(3 f))` people in places tagged tavern, market, dock or gate get it as a `Held` fact, with `conf = d` and source `ToldBy(traveller)`. NPC_MINDS's gossip pass then carries it round the town.
- Optionally, a traveller NPC walks in along the arrival edge's road carrying it, so news arrives with a person.

**Confidence and detail are one number.** Each person-to-person telling inside the bubble is one retelling: it multiplies `d` by 0.92 and draws drift from a hash of (story, teller, listener). This replaces NPC_MINDS 9.2's x0.8 per hop, which would leave a story at 0.11 after crossing a town in ten hops.

**Fast-forward.** `StopOn` could gain `news: Option<f32>`, which stops a sleep when a story with `h >=` that value is released at the player's node (a herald at the gate). Arrivals released during a skip are queued for the next hourly pass, so nothing is lost.

**News changes the far simulation (hooks):**
- A plague story raises emigration from the nodes that hear it.
- A bandits-on-the-road story lowers traffic on that edge. That raises its `tau` at the next rebuild, so bad news slows news.
- A battle story moves faction regard (section 6).

**Engine and game.** The engine provides the network, spread, blur and drift machinery, reputation sums and opinion dynamics. The game registers predicates with `W_pred`, saliences, escalations and issue charges, the ladders' words, the issues, the channel parameters and the reputation axes.

**Files (proposed):**
- `src/region/news.rs`: `Story`, `Slot`, `Arrival`, `Variant`, `Network`, `Node`, `Edge`, `News` with `open`, `spread`, `release`, `daily`, `heard_at`, `telling`.
- `src/region/opinion.rs`: `IssueId`, `Group`, the daily update.

The L0 bridge lives in `life/fact.rs` (NPC_MINDS). Rust note: the queue of arrivals is a `BinaryHeap<Reverse<(time, ArrivalId)>>`. Rust's heap gives the largest item first, and wrapping in `Reverse` flips that, which is the usual way to get a min-heap (Python's `heapq` is a min-heap already).

**Cost (estimate):**
- A network of 1 000 nodes and 4 000 edges: a story's Dijkstra stops at `h < 1`, under 0.1 ms even for one that crosses a province.
- Opinion: 1 000 nodes x 4 groups x 16 issues, about 1.3 ms per game day.
- Reputation (32 tracked subjects per node plus the player) x 7 axes: about 1 ms per game day.
- Memory: about 1 MB.

All of it is negligible next to L0, even during a fast-forward.

## 8. Worked example

**The event.** Varnakkel, day 40 at 20:00 (`t` = 40.83). A Templar patrol under Brother Aldric burns 12 houses hunting a heretic. `W` = 8 x (1 + log10 13) x 1.2 (Templar fame 0.2) = 20, so `t_half` = 2 x (1 + log10 20) = 4.6 days.

**The network:**

| Edge | Length | rho | Road | tau (days) | A | N_edge |
|---|---|---|---|---|---|---|
| Varnakkel (P 1200) - Fennick (P 400) | 18 km | 40 | yes | 0.42 | 0.40 | 2.7 |
| Fennick - Hollin (P 60) | 12 km moor | 3 | no | 1.25 | 0.70 | 0.69 |
| Fennick - Saltmere (P 5000) | 50 km | 60 | yes | 1.07 | 0.83 | 10.0 |
| Varnakkel - Saltmere, sea lane | 60 km | - | - | 1.2 (0.5 sail + 0.7 wait) | 0.40 | 1 |
| Hollin - Gorse (P 30) | 15 km moor | 2 | no | 1.83 | 0.91 | 0.83 |

Retellings inside each node before a story leaves (`N_node`): Varnakkel 2.81, Fennick 1.58, Hollin 0.38.

**Arrivals:**

| Node | Arrives | h | N | d | f | Heard for |
|---|---|---|---|---|---|---|
| Varnakkel | day 40 20:00 | 20.0 | 0 | 1.00 | 0.98 | 20 days |
| Fennick | day 41 06:00 | 12.6 | 5.5 | 0.63 | 0.92 | 17 days |
| Saltmere by sea (first) | day 42 00:45 | 11.2 | 3.8 | 0.73 | 0.89 | 16 days |
| Saltmere by road (ignored, later) | day 42 07:40 | 4.7 | 17.1 | 0.24 | | |
| Hollin | day 42 12:15 | 5.2 | 7.8 | 0.52 | 0.65 | 11 days |
| Gorse | day 44 08:10 | 1.6 | 9.0 | 0.47 | 0.27 | 3 days |

Beyond Gorse, `h` would fall below 1, so the story ends there. The sea lane beats the road to Saltmere by seven hours, and with far better detail: the busy road would have told it seventeen times.

**Drift in this seed.** The expected count of drifts on "how many" is `N x 0.07`: 0.39 by Fennick, 0.27 by Saltmere.
- At Fennick the hash draws one drift, x1.25, so 15 houses.
- Hollin inherits that and draws one more, x1.3, so 19.5 against a cap of 36.
- Saltmere draws none.

**What the player hears** (lines through NPC_MINDS 15.4, hedged by `d`):
- **Saltmere** (d 0.73, all slots exact; **news**, from the sailors who were there): "Boat in from Varnakkel. Brother Aldric's Templars burned twelve houses there, night before last."
- **Fennick** (d 0.63; who and when still exact, how many rounded; **rumour**): "They say Aldric's Templars burned fifteen houses at Varnakkel yesterday evening."
- **Hollin** (d 0.52; who at rung 1, where still named; **rumour**): "They say a Templar patrol burned twenty houses down in Varnakkel, two days back."
- **Gorse** (d 0.47; where drops to rung 1; f 0.27, so only a quarter have heard; **rumour**): "Heard some Templars burned twenty houses down on the coast, a few days back."

**Reputation, violence axis** (`v` = +0.5):
- Fennick and Saltmere: Aldric +0.46 and +0.45, the Templars +0.23 and +0.22.
- Hollin: only the Templars, +0.16.
- Gorse: the Templars, +0.07.

In Hollin, Aldric himself is unknown.

**Belief in Saltmere**, on the issue "the Templars are just", with story charge `q` = -0.4 and `f x d` = 0.65. Orthodox and old creed are opposed (`r` = -0.5). Merchants are friendly to the orthodox (+0.5) and mildly to the old creed (+0.2).

| Group (share) | x, c before | after the story | day 7 | day 14 |
|---|---|---|---|---|
| orthodox (0.5) | +0.30, 0.40 | +0.20 | +0.21, 0.41 | +0.25, 0.42 |
| merchants (0.2) | 0.00, 0.20 | -0.21 | -0.09, 0.23 | -0.02, 0.25 |
| old creed (0.3) | -0.20, 0.30 | -0.42 | -0.52, 0.37 | -0.62, 0.42 |

What the numbers show:
- The story dents the Templars' standing in every group at first.
- Within a fortnight the orthodox have won half the dent back. Their opponents' anger pushes them.
- The old creed has hardened well past the story's own push.
- The merchants have drifted back towards their orthodox friends.
- The gap between opposed groups grows from 0.50 to 0.87, and both grow more certain. Same side converges; opposed sides diverge and strengthen.

## 9. Build order

Each step has the test that proves it.
1. **N-a, network and spread on a hand-built graph** (S, needs nothing else). Tests:
   - the section 8 table is reproduced to two decimals
   - doubling the density of a corridor shortens `tau` and lowers `d`
   - on a chain of 10 km edges at density 10, a `W` 2 story is never heard two edges out
   - the same seed gives the same variants
   - drift never breaks a cap, and "what" never changes
2. **N-b, telling** (S, after N-a). Ladders, hedges and the news or rumour label. Tests:
   - Hollin's slots sit at the rungs shown in section 8
   - an official channel is labelled news at any `d`
3. **N-c, reputation from arrivals** (S). Tests:
   - who at rung 1 credits the faction only, and rung 2 credits nobody
   - the 60-day half-life holds
   - fame raises the next story's `W`
4. **N-d, opinion groups** (S-M). Tests:
   - in a two-group town with `r` = 1, the gap halves within 5 days
   - opposed groups' gap grows and their `c` rises
   - a town of one mind relaxes `c` to `c_0`
   - values stay in range
5. **N-e, the bubble bridge** (M, after NPC_MINDS N5 and N7). Tests:
   - an arrival seeds tavern people and is town-wide in about two game days
   - a fact witnessed in the bubble opens a story
   - demote then promote keeps each group's mean `x` within 0.01
6. **N-f, network from the region layer** (M, after R1). Test: edges are rebuilt from density, and cutting a road lengthens arrivals beyond it.
7. **N-g, far hooks** (S each). Plague and emigration, bandits and traffic, unrest and the director.

**Limits of this version.** First passage only: a later, clearer telling does not correct an earlier one. A second arrival could add heat and raise `d` to the larger of the two; that is a later step if play wants it. Stories do not merge: two fires in one town are two stories.

## 10. Questions for Mickey

1. How fast should gossip travel: historical speeds (about 40 km a day on a road, 5 across empty land), twice that for play, or a setting per game?
2. How much may a rumour invent? Numbers up to x2 and never a wrong name; up to x3 and sometimes the wrong culprit of the same role (my default); or up to x5 with wrong culprits and escalated deeds?
3. Should a deed blurred to "a Templar" stain the whole faction's reputation far away, or should only named people ever gain reputation?
4. How should far news reach the player: only through NPC talk, a log line labelled News or Rumour for everything, or a log line only for official news (heralds, letters, notices) with the rest through talk?
5. When opposed groups in one town polarise past a threshold, should that emit unrest (riots, expulsions) for the director to budget, or should opinions only drift?
6. Should the player's reputation in a town be held per faction group (the orthodox admire you while the old creed hates you), or one value per town?
