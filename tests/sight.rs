//! Seeing far: horizon, hull down, high ground, haze, glass and night.

use tachyon::sight::{horizon, look, Air, Murk, Obstacle, Target};

fn ship(x: f32, y: f32) -> Target {
    // A frigate: hull to 5 m, masts to 45 m, 40 m long, dark against the sea.
    Target { x, y, bottom: 0.0, top: 45.0, hull: 5.0, breadth: 30.0, contrast: 0.7 }
}

fn sea(_: f32, _: f32) -> f32 {
    -50.0
}

#[test]
fn the_horizon_is_where_it_should_be() {
    let h = horizon(1.7);
    assert!((4800.0..5200.0).contains(&h), "from a man's eye: {h:.0} m");
    let m = horizon(30.0);
    assert!((20_000.0..22_000.0).contains(&m), "from the masthead: {m:.0} m");
}

#[test]
fn far_off_she_is_hull_down_from_the_deck_but_hull_up_from_the_masthead() {
    let air = Air::default();
    let t = ship(0.0, -18_000.0);
    let deck = look((0.0, 0.0, 5.0), &t, &air, 1.0, sea);
    assert!(deck.hull_down, "from the deck her hull is under the horizon (hidden below {:.1} m)", deck.hidden_below);
    assert!(deck.showing > 0.0, "but her topsails show");
    assert!((deck.bearing).abs() < 1e-3 || (deck.bearing - std::f32::consts::TAU).abs() < 1e-3, "bearing north");
    let mast = look((0.0, 0.0, 30.0), &t, &air, 1.0, sea);
    assert!(!mast.hull_down, "the masthead sees her hull (hidden below {:.1} m)", mast.hidden_below);
    // Far enough and even her mastheads go.
    let gone = look((0.0, 0.0, 5.0), &ship(0.0, -40_000.0), &air, 1.0, sea);
    assert_eq!(gone.showing, 0.0);
}

#[test]
fn an_island_hides_what_is_behind_it() {
    let air = Air::default();
    let island = |x: f32, y: f32| if (x * x + (y + 3000.0).powi(2)).sqrt() < 400.0 { 60.0 } else { -40.0 };
    let behind = look((0.0, 0.0, 5.0), &ship(0.0, -6000.0), &air, 1.0, island);
    assert_eq!(behind.showing, 0.0, "behind a 60 m island a 45 m mast is hidden");
    let beside = look((0.0, 0.0, 5.0), &ship(2000.0, -6000.0), &air, 1.0, island);
    assert!(beside.showing > 40.0 && beside.clarity > 1.0, "clear of it she is in plain sight");
}

#[test]
fn another_ship_can_mask_a_ship() {
    let mut air = Air::default();
    air.obstacles.push(Obstacle { x: 0.0, y: -1000.0, radius: 20.0, top: 45.0 });
    let masked = look((0.0, 0.0, 5.0), &ship(0.0, -3000.0), &air, 1.0, sea);
    assert!(masked.hidden_below > 25.0, "a nearer ship's canvas covers most of her: hidden below {:.1}", masked.hidden_below);
}

#[test]
fn haze_fog_and_smoke_wash_her_out() {
    let t = ship(0.0, -4000.0);
    let clear = look((0.0, 0.0, 5.0), &t, &Air::default(), 1.0, sea);
    assert!(clear.clarity > 5.0, "plain on a clear day: {:.1}", clear.clarity);
    let hazy = look((0.0, 0.0, 5.0), &t, &Air { visibility: 3000.0, ..Air::default() }, 1.0, sea);
    assert!(hazy.clarity < 1.0, "lost in haze beyond the visibility: {:.2}", hazy.clarity);
    let mut smoky = Air::default();
    smoky.murk.push(Murk { x: 0.0, y: -2000.0, radius: 150.0, top: 60.0, visibility: 30.0 });
    let s = look((0.0, 0.0, 5.0), &t, &smoky, 1.0, sea);
    assert!(s.clarity < 0.1, "a bank of powder smoke between: {:.3}", s.clarity);
    // Low fog: a masthead above it sees over it.
    let mut fog = Air::default();
    fog.murk.push(Murk { x: 0.0, y: -2000.0, radius: 500.0, top: 8.0, visibility: 50.0 });
    let over = look((0.0, 0.0, 30.0), &t, &fog, 1.0, sea);
    assert!(over.clarity > 1.0, "the lookout aloft sees over a low fog: {:.2}", over.clarity);
}

#[test]
fn a_glass_brings_up_a_small_far_sail_and_night_hides_it() {
    // A boat 16 km off, just over the horizon from 12 m up: a speck.
    let boat = Target { x: 0.0, y: -16_000.0, bottom: 0.0, top: 6.0, hull: 1.0, breadth: 4.0, contrast: 0.5 };
    let air = Air::default();
    let eye = look((0.0, 0.0, 12.0), &boat, &air, 1.0, sea);
    let glass = look((0.0, 0.0, 12.0), &boat, &air, 10.0, sea);
    assert!(eye.clarity < 1.0 && glass.clarity > 1.0, "too small by eye, made out through the glass: {:.2} vs {:.2}", eye.clarity, glass.clarity);
    let night = look((0.0, 0.0, 12.0), &boat, &Air { light: 0.05, ..Air::default() }, 10.0, sea);
    assert!(night.clarity < 0.1 * glass.clarity, "but not much at night: {:.3}", night.clarity);
}

#[test]
fn a_heavy_sea_raises_the_horizon_and_hides_a_boat_in_the_troughs() {
    let boat = Target { x: 0.0, y: -3000.0, bottom: 0.0, top: 1.5, hull: 1.5, breadth: 5.0, contrast: 0.6 };
    let calm = look((0.0, 0.0, 5.0), &boat, &Air::default(), 1.0, sea);
    let rough = look((0.0, 0.0, 5.0), &boat, &Air { waves: 4.0, ..Air::default() }, 1.0, sea);
    assert!(calm.showing > 1.0, "in a calm she shows: {:.2}", calm.showing);
    assert!(rough.showing < 0.5 * calm.showing, "in a big sea she is mostly hidden: {:.2}", rough.showing);
}
