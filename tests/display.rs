//! The display layer: pure ASCII really is pure, and colour schemes map sanely.

use tachyon::display::{ascii, xterm256, Ink};
use tachyon::prelude::*;

#[test]
fn every_standard_material_glyph_folds_to_real_ascii() {
    let mats = Materials::standard();
    for i in 0..mats.len() {
        let m = mats.get(i as MatId);
        for ch in std::iter::once(m.wall_glyph).chain(m.floor_glyphs.iter().copied()) {
            let a = ascii(ch);
            assert!(a.is_ascii() && a != '?', "{}: '{ch}' folds to '{a}'", m.name);
        }
    }
}

#[test]
fn a_drawn_world_reaches_the_screen_as_seven_bit_ascii() {
    // A walled room with water, a ramp and a stair, drawn both narrow and wide.
    let mats = Materials::standard();
    let (granite, grass, water) = (mats.id("granite"), mats.id("grass"), mats.id("water"));
    let mut world = World::new(24, 16, 3, mats);
    for y in 0..16 {
        for x in 0..24 {
            world.set_tile(Pos::new(x, y, 0), Tile::Wall, granite);
            let edge = x == 2 || y == 2 || x == 20 || y == 12;
            let tile = if edge && (2..=20).contains(&x) && (2..=12).contains(&y) { Tile::Wall } else { Tile::Floor };
            world.set_tile(Pos::new(x, y, 1), tile, granite);
            if tile == Tile::Floor {
                world.set_tile(Pos::new(x, y, 1), Tile::Floor, grass);
            }
        }
    }
    world.set_tile(Pos::new(6, 6, 1), Tile::Ramp, granite);
    for x in 8..12 {
        world.set_tile(Pos::new(x, 8, 0), Tile::Floor, granite);
        world.set_tile(Pos::new(x, 8, 1), Tile::Floor, granite);
        world.add_fluid(Pos::new(x, 8, 1), water, FULL);
    }
    let scene = Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 1);
    let d = Display::default();
    for wide in [false, true] {
        let mut view = MapView::new();
        view.center = Pos::new(12, 8, 1);
        view.wide = wide;
        let mut f = Frame::new(60, 20);
        let area = f.rect();
        view.draw(&scene, &mut f, area);
        f.border(Rect::new(0, 0, 60, 20), Rgb::WHITE, Rgb::BLACK, "map");
        d.apply(&mut f);
        for c in f.cells() {
            assert!(c.ch.is_ascii() && c.ch != '?', "'{}' reached the screen", c.ch);
        }
    }
}

#[test]
fn terminal_scheme_keeps_hue_and_hands_dark_backgrounds_to_the_terminal() {
    let d = Display { scheme: Scheme::Terminal, ..Display::default() };
    assert_eq!(d.ink(Rgb(10, 10, 14), true), Ink::Default);
    // A dim red stays red rather than turning black; a bright one is bright red.
    assert_eq!(d.ink(Rgb(110, 20, 20), false), Ink::Ansi(1));
    assert_eq!(d.ink(Rgb(240, 60, 60), false), Ink::Ansi(9));
    assert_eq!(d.ink(Rgb(30, 50, 140), false), Ink::Ansi(4));
    assert_eq!(d.ink(Rgb(40, 90, 220), false), Ink::Ansi(12));
    assert_eq!(d.ink(Rgb(60, 170, 60), false), Ink::Ansi(10));
}

#[test]
fn xterm256_hits_cube_corners_and_greys() {
    assert_eq!(xterm256(Rgb(255, 0, 0)), 196);
    assert_eq!(xterm256(Rgb(0, 0, 0)), 16);
    assert_eq!(xterm256(Rgb(128, 128, 128)), 244);
}

#[test]
fn monochrome_schemes_keep_dim_things_visible() {
    for scheme in [Scheme::Phosphor, Scheme::Amber, Scheme::Paper] {
        let d = Display { scheme, ..Display::default() };
        let (Ink::Rgb(dim), Ink::Rgb(bg)) = (d.ink(Rgb(30, 30, 30), false), d.ink(Rgb(0, 0, 0), true)) else { panic!() };
        let gap = (dim.luma() - bg.luma()).abs();
        assert!(gap > 30.0, "{}: dim text too close to the background ({gap})", scheme.name());
    }
}
