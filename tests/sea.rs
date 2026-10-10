//! The sea as fields: tides, waves, shelter, currents and whirlpools.

use tachyon::sea::{Sea, SeaSettings, Whirlpool, M2};

fn open_sea(settings: SeaSettings) -> Sea {
    Sea::new(64, 64, settings, |_, _| 40.0)
}

#[test]
fn tides_rise_and_fall_with_springs_and_neaps() {
    let mut s = open_sea(SeaSettings { tide_range: 4.0, ..Default::default() });
    let (mut hi, mut lo) = (f32::MIN, f32::MAX);
    let mut daily_ranges = Vec::new();
    for day in 0..15 {
        let (mut dh, mut dl) = (f32::MIN, f32::MAX);
        for k in 0..288 {
            s.set_time((day * 86_400 + k * 300) as f64);
            let t = s.tide();
            dh = dh.max(t);
            dl = dl.min(t);
            // The stream floods while the tide rises.
            let ahead = s.tide_at(s.time + 60.0);
            if (ahead - t).abs() > 1e-3 {
                assert_eq!(s.stream() > 0.0, ahead > t, "flood with a rising tide (day {day}, step {k})");
            }
        }
        hi = hi.max(dh);
        lo = lo.min(dl);
        daily_ranges.push(dh - dl);
    }
    assert!(hi <= 2.01 && lo >= -2.01, "spring tides reach half the range: {lo:.2}..{hi:.2}");
    assert!(hi > 1.8, "springs come round within a fortnight");
    let (rmax, rmin) = daily_ranges.iter().fold((0.0f32, f32::MAX), |(a, b), &r| (a.max(r), b.min(r)));
    assert!(rmin < 0.55 * rmax, "neaps are much smaller than springs: {rmin:.2} vs {rmax:.2}");
    let _ = M2;
}

#[test]
fn wind_builds_a_sea_and_swell_outlasts_it() {
    let mut s = open_sea(SeaSettings::default());
    let wind = (12.0, 0.0);
    // From a calm: a sea takes hours to build.
    for _ in 0..10 {
        s.step(60.0, wind);
    }
    let early = s.state().hs;
    for _ in 0..(14 * 60) {
        s.step(60.0, wind);
    }
    let built = s.state().hs;
    let mut eq = open_sea(SeaSettings::default());
    eq.settle(wind);
    let full = eq.state().hs;
    assert!(full > 2.0 && full < 5.0, "a 12 m/s wind over open sea makes ~3 m waves, got {full:.2}");
    assert!(early < 0.5 * full, "ten minutes is not enough: {early:.2} of {full:.2}");
    assert!(built > 0.8 * full, "fourteen hours nearly is: {built:.2} of {full:.2}");
    assert!((s.state().dir).abs() < 0.3, "the waves run with the wind (east)");

    // The wind drops: the long waves linger as swell.
    for _ in 0..(6 * 60) {
        s.step(60.0, (0.0, 0.0));
    }
    let swell = s.state();
    assert!(swell.hs > 0.3 * built && swell.hs < 0.95 * built, "swell after six hours: {:.2} of {built:.2}", swell.hs);
    assert!(swell.tp > 6.0, "what is left is long-period swell: {:.1} s", swell.tp);
    for _ in 0..(72 * 60) {
        s.step(60.0, (0.0, 0.0));
    }
    assert!(s.state().hs < 0.15 * built, "three days of calm and the sea goes down");
}

#[test]
fn surface_heights_match_the_sea_state() {
    let mut s = open_sea(SeaSettings::default());
    s.settle((10.0, 0.0));
    let hs = s.state().hs;
    let (mut sum2, mut n) = (0.0f64, 0);
    for k in 0..4000 {
        let (x, y) = ((k % 63) as f32 * 13.0 + 3.0, (k / 63) as f32 * 11.0 + 5.0);
        let z = s.height(x, y) as f64;
        sum2 += z * z;
        n += 1;
    }
    let measured = 4.0 * (sum2 / n as f64).sqrt() as f32;
    assert!((measured - hs).abs() < 0.3 * hs, "Hs from the surface {measured:.2} vs spectrum {hs:.2}");
}

#[test]
fn an_island_shelters_its_lee() {
    let mut s = Sea::new(200, 200, SeaSettings::default(), |x, y| {
        let (dx, dy) = (x / 16.0 - 100.0, y / 16.0 - 100.0);
        if dx * dx + dy * dy < 20.0 * 20.0 { -10.0 } else { 30.0 }
    });
    s.settle((12.0, 0.0)); // blowing toward the east
    let windward = s.wave_scale(16.0 * 60.0, 16.0 * 100.0);
    let lee = s.wave_scale(16.0 * 125.0, 16.0 * 100.0);
    assert!(windward > 0.9, "open water to windward: {windward:.2}");
    assert!(lee < 0.35, "calm in the island's lee: {lee:.2}");
    assert_eq!(s.wave_scale(16.0 * 100.0, 16.0 * 100.0), 0.0, "no waves on land");
}

#[test]
fn waves_steepen_and_break_on_a_beach() {
    let mut s = Sea::new(200, 20, SeaSettings::default(), |x, _| 30.0 - x * 0.02); // shoals to the east
    s.settle((12.0, 0.0));
    let deep = s.wave_scale(160.0, 160.0);
    // Depth ~2 m: breaking.
    let x_surf = (30.0 - 2.0) / 0.02;
    assert!(s.breaking(x_surf, 160.0), "surf in two metres of water");
    assert!(!s.breaking(160.0, 160.0), "no surf offshore");
    let h = s.water_depth(x_surf, 160.0);
    assert!(s.wave_scale(x_surf, 160.0) * s.state().hs <= 0.56 * h, "broken waves are depth-limited");
    assert!(deep > 0.0);
}

/// A sea with a north-south wall of land at x 54..74 (cells), pierced by a
/// strait at y 28..36, and a lake enclosed by land in the north-west.
fn strait() -> Sea {
    let settings = SeaSettings { tidal_stream: (0.5, 0.0), tide_range: 2.0, ..Default::default() };
    Sea::new(128, 64, settings, |x, y| {
        let (cx, cy) = ((x / 16.0) as i32, (y / 16.0) as i32);
        if (54..74).contains(&cx) && !(28..36).contains(&cy) {
            return -5.0;
        }
        // The lake and its shore.
        if (5..20).contains(&cx) && (3..15).contains(&cy) {
            return if (7..18).contains(&cx) && (5..13).contains(&cy) { 8.0 } else { -3.0 };
        }
        20.0
    })
}

#[test]
fn the_tidal_stream_races_through_a_strait() {
    let mut s = strait();
    // Find mid-flood.
    let mut t = 0.0;
    while s.stream() < 0.95 {
        t += 300.0;
        s.set_time(t);
    }
    let (sx, sy) = s.current(16.0 * 64.0, 16.0 * 32.0);
    let (ox, _) = s.current(16.0 * 30.0, 16.0 * 50.0);
    assert!(sx > 3.0 * 0.5, "fast in the strait: {sx:.2} m/s");
    assert!(sy.abs() < 0.3 * sx, "and along it");
    assert!(ox > 0.0 && ox < 0.75, "slack-ish in the open water before the wall: {ox:.2}");
    assert_eq!(s.current(16.0 * 64.0, 16.0 * 10.0), (0.0, 0.0), "no current on land");
    let (lx, ly) = s.current(16.0 * 12.0, 16.0 * 9.0);
    assert!(lx.abs() + ly.abs() < 1e-4, "an enclosed lake feels no sea current");
    // Half a tide later the stream runs the other way.
    s.set_time(t + M2 as f64 / 2.0);
    assert!(s.current(16.0 * 64.0, 16.0 * 32.0).0 < -1.0, "the ebb runs back through");
}

#[test]
fn headlands_shed_eddies_while_the_stream_runs() {
    let mut s = strait();
    assert!(!s.sites.is_empty(), "the strait's mouths are headlands");
    s.settings.eddy_threshold = 0.6;
    let mut seen = 0;
    let mut max_at_slack: f32 = 0.0;
    for _ in 0..(13 * 60) {
        s.step(60.0, (0.0, 0.0));
        seen = seen.max(s.whirlpools.len());
        if s.stream().abs() < 0.05 {
            for w in &s.whirlpools {
                max_at_slack = max_at_slack.max(w.now);
            }
        }
    }
    assert!(seen > 0, "a tide's worth of hard running sheds eddies");
    assert!(max_at_slack < 0.1, "eddies fall still at slack water: {max_at_slack:.2}");
}

#[test]
fn a_whirlpool_turns_draws_in_and_dips() {
    let mut s = open_sea(SeaSettings { tidal_stream: (1.0, 0.0), ..Default::default() });
    let (cx, cy) = (512.0, 512.0);
    s.add_whirlpool(Whirlpool::new(cx, cy, 30.0, 3.0, 1.0));
    let mut t = 0.0;
    while s.stream() < 0.95 {
        t += 300.0;
        s.set_time(t);
    }
    s.step(1.0, (0.0, 0.0));
    let tide = s.tide();
    // East of the centre, a clockwise (on screen, y down) turn runs south.
    let (_, vy) = s.current(cx + 30.0, cy);
    assert!(vy > 2.0, "turning hard at the rim: {vy:.2}");
    let (_, vy_w) = s.current(cx - 30.0, cy);
    assert!(vy_w < -2.0, "and the other way on the far side");
    // Inward pull: north of the centre, something drifts south.
    let w = s.whirlpools[0];
    let (_, iy, _) = w.effect(0.0, -60.0);
    assert!(iy > 0.0, "it draws things in");
    assert!(s.height(cx, cy) < tide - 0.5, "the surface dips into a funnel");
    // At slack water it is gone.
    while s.stream().abs() > 0.03 {
        t += 60.0;
        s.set_time(t);
    }
    s.step(1.0, (0.0, 0.0));
    assert!(s.whirlpools[0].now < 0.05, "a tidal whirlpool sleeps at slack water");
}

#[test]
fn the_same_seed_gives_the_same_sea() {
    let mk = || {
        let mut s = strait();
        s.settle((8.0, 3.0));
        for _ in 0..30 {
            s.step(60.0, (8.0, 3.0));
        }
        s
    };
    let (a, b) = (mk(), mk());
    for k in 0..50 {
        let (x, y) = (k as f32 * 37.0, k as f32 * 19.0);
        assert_eq!(a.height(x, y), b.height(x, y));
        assert_eq!(a.current(x, y), b.current(x, y));
    }
}
