//! The Tokamak file formats (`.tlib`, `.tpart`, `.tart`) load and write back byte for byte.

use tachyon::library::{library_text, parse_library, Art, Part, SEED_LIBRARY};

const TLIB: &str = "tachyon-library 1
# The sample library: materials (wall and floor glyphs) and kinds (things).
material stone block | # . | 178 176 168 | 52 50 48 | stone masonry
material water | ~ ~ | 96 150 196 | 16 38 64 | liquid
kind mast | | | 200 170 110 | 0 0 0 | ship
kind door | ' | 200 150 90 | 0 0 0 | opening
";

const TPART: &str = "tachyon-blueprint 1
name hut
size 3 2 1
key # wall stone block
key . floor stone block
level 0
###
#.#
end
socket 1 0 0 n door
socket 2 1 0 down curtain-wall
";

const TART: &str = "tachyon-art 1
name sign
size 2 1
color a 220 180 90
color b 0 0 0
color c 110 170 220
glyphs
T~
ink
ac
paper
bb
end
";

#[test]
fn library_file_round_trips_byte_for_byte() {
    let lib = parse_library(TLIB);
    assert_eq!(lib.len(), 4);
    assert_eq!(library_text("sample", &lib), TLIB);
}

#[test]
fn part_file_round_trips_byte_for_byte() {
    let p = Part::from_text(TPART).unwrap();
    assert_eq!((p.bp.w, p.bp.h, p.bp.d, p.sockets.len()), (3, 2, 1, 2));
    assert_eq!(p.text(), TPART);
}

#[test]
fn art_file_round_trips_byte_for_byte() {
    let a = Art::from_text(TART).unwrap();
    assert_eq!((a.w, a.h, a.cells[1].ch), (2, 1, '~'));
    assert_eq!(a.text(), TART);
}

#[test]
fn the_seed_library_parses_and_rewrites_stably() {
    let lib = parse_library(SEED_LIBRARY);
    let once = library_text("seed", &lib);
    assert_eq!(library_text("seed", &parse_library(&once)), once);
}
