//! Rafts on water, and the direction pad.

use std::time::{Duration, Instant};
use tachyon::prelude::*;
use tachyon::sim::Source;

/// A granite basin: floor at z = 0, water full on z = 1, walls up to the top.
fn pond(w: i32, h: i32) -> (Scene, KindId) {
    let mats = Materials::standard();
    let (granite, water) = (mats.id("granite"), mats.id("water"));
    let mut world = World::new(w, h, 4, mats);
    for y in 0..h {
        for x in 0..w {
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            for z in 0..4 {
                if z == 0 || edge {
                    world.set_tile(Pos::new(x, y, z), Tile::Wall, granite);
                }
            }
            if !edge {
                world.set_fluid(Pos::new(x, y, 1), water, FULL);
            }
        }
    }
    let mut things = Things::new();
    let raft = things.add_kind(ThingKind::new("raft", '=', Rgb(190, 140, 80), Layer::Furniture).deck());
    (Scene::new(world, things, Clock::new(1.0, 12.0), 3), raft)
}

fn launch(scene: &mut Scene, kind: KindId, at: Pos) {
    let v = Vessel::build("raft", &mut scene.things, at, &["===", "==="], |c| (c == '=').then_some(kind));
    scene.vessels.push(v);
}

#[test]
fn raft_floats_on_top_of_full_water() {
    let (mut scene, kind) = pond(20, 12);
    launch(&mut scene, kind, Pos::new(5, 5, 2));
    for _ in 0..50 {
        scene.tick();
    }
    let v = &scene.vessels[0];
    assert_eq!(v.z, 2, "a raft on a full level-1 pool sits on level 2");
    assert_eq!(v.state, Afloat::Afloat);
}

#[test]
fn paddled_raft_conserves_water_and_carries_its_passenger() {
    let (mut scene, kind) = pond(30, 12);
    let water = scene.world.mats.id("water");
    let you = scene.things.add_kind(ThingKind::new("you", '@', Rgb::WHITE, Layer::Creature));
    launch(&mut scene, kind, Pos::new(3, 4, 2));
    let me = scene.things.spawn(you, Pos::new(4, 5, 2));
    let before = scene.world.total_fluid(water);
    for t in 0..300 {
        if t % 4 == 0 {
            scene.vessels[0].push(0.03, 0.0);
        }
        scene.tick();
    }
    let v = &scene.vessels[0];
    assert!(v.origin().x >= 10, "raft barely moved: {:?}", v.origin());
    let p = scene.things.get(me).unwrap().pos();
    assert!(v.contains(p), "passenger at {p:?} fell off the raft at {:?}", v.origin());
    // Water evaporates only from films of 1-2 units, a slow process; the
    // bow wave itself must not create or destroy any.
    let after = scene.world.total_fluid(water);
    assert!(before - after < 20 && after <= before, "water {before} -> {after}");
}

#[test]
fn raft_stops_at_the_bank() {
    let (mut scene, kind) = pond(14, 10);
    launch(&mut scene, kind, Pos::new(3, 3, 2));
    for _ in 0..300 {
        scene.vessels[0].push(0.05, 0.0);
        scene.tick();
    }
    let v = &scene.vessels[0];
    assert_eq!(v.origin().x, 14 - 1 - 3, "should rest against the east wall");
}

#[test]
fn current_carries_a_raft_downstream() {
    // A long channel with a spring at the west end and a drain at the east.
    let (mut scene, kind) = pond(40, 7);
    let water = scene.world.mats.id("water");
    for y in 1..6 {
        scene.sim.sources.push(Source { pos: Pos::new(1, y, 1), mat: water, rate: 8 });
        scene.sim.sources.push(Source { pos: Pos::new(38, y, 1), mat: water, rate: -8 });
    }
    for _ in 0..400 {
        scene.tick();
    }
    let (fx, _) = scene.world.current(Pos::new(30, 3, 1));
    assert!(fx > 0.0, "no eastward current in the channel ({fx})");
    launch(&mut scene, kind, Pos::new(4, 2, 2));
    let x0 = scene.vessels[0].x;
    for _ in 0..1500 {
        scene.tick();
    }
    let v = &scene.vessels[0];
    assert!(v.x > x0 + 5.0, "raft did not drift downstream: {x0} -> {}", v.x);
}

#[test]
fn dirpad_taps_holds_and_diagonals() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);

    // A tap: one step, after the chord window, even if already released.
    let mut pad = DirPad::new();
    let c = pad.chord.as_millis() as u64;
    pad.press(Key::Up, (0, -1), ms(0));
    pad.release(Key::Up);
    assert_eq!(pad.update(ms(10)), None);
    assert_eq!(pad.update(ms(c)), Some((0, -1)));
    assert_eq!(pad.update(ms(500)), None);

    // Two keys pressed together: one diagonal step, then diagonal walking.
    // (Holding only works once the pad has seen a key go up at all.)
    let mut pad = DirPad::new();
    pad.release(Key::Tab);
    pad.press(Key::Up, (0, -1), ms(0));
    pad.press(Key::Right, (1, 0), ms(50)); // a human "together": 50 ms apart
    assert_eq!(pad.update(ms(c)), Some((1, -1)));
    assert_eq!(pad.update(ms(c + 100)), None);
    assert_eq!(pad.update(ms(c + 220)), Some((1, -1)));
    pad.release(Key::Right);
    assert_eq!(pad.update(ms(c + 330)), Some((0, -1)));
    pad.release(Key::Up);
    assert_eq!(pad.update(ms(1000)), None);

    // Walking up, then Right joins: the diagonal comes within the chord
    // window, not after a whole repeat.
    let mut pad = DirPad::new();
    pad.release(Key::Tab);
    pad.press(Key::Up, (0, -1), ms(0));
    assert_eq!(pad.update(ms(c)), Some((0, -1)));
    pad.press(Key::Right, (1, 0), ms(c + 10));
    assert_eq!(pad.update(ms(c + 10 + c)), Some((1, -1)));

    // A terminal that never reports releases: every press is one step.
    let mut pad = DirPad::new();
    for i in 0..3 {
        pad.press(Key::Left, (-1, 0), ms(i * 200));
        assert_eq!(pad.update(ms(i * 200 + c)), Some((-1, 0)));
    }
    assert_eq!(pad.update(ms(1000)), None);
}
