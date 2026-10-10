//! What the editor offers: rock types, biomes, feature kinds and the
//! furniture you can place. A game would keep its own version of this.

use tachyon::prelude::*;
use tachyon::worldgen::{BiomeDef, GenSettings};

pub const ROCKS: [&str; 7] = ["granite", "basalt", "limestone", "sandstone", "marble", "chalk", "slate"];

/// A biome: how it looks on the atlas, and what it means on the ground.
pub struct Biome {
    pub name: &'static str,
    pub glyph: char,
    pub color: Rgb,
    /// Surface and subsoil material names ("" surface = bare rock).
    pub surface: &'static str,
    pub subsoil: &'static str,
    pub soil_depth: i32,
    pub trees: f32,
    pub roughness: f32,
    pub puddles: f32,
}

pub const BIOMES: [Biome; 10] = [
    Biome { name: "ocean", glyph: '≈', color: Rgb(40, 90, 180), surface: "sand", subsoil: "sand", soil_depth: 2, trees: 0.0, roughness: 0.5, puddles: 0.0 },
    Biome { name: "beach", glyph: '.', color: Rgb(214, 196, 140), surface: "sand", subsoil: "sand", soil_depth: 3, trees: 0.0, roughness: 0.3, puddles: 0.0 },
    Biome { name: "grassland", glyph: '"', color: Rgb(100, 170, 70), surface: "grass", subsoil: "soil", soil_depth: 2, trees: 0.006, roughness: 1.0, puddles: 0.0 },
    Biome { name: "forest", glyph: '♣', color: Rgb(40, 120, 50), surface: "grass", subsoil: "soil", soil_depth: 2, trees: 0.09, roughness: 1.2, puddles: 0.0 },
    Biome { name: "marsh", glyph: 'ψ', color: Rgb(110, 130, 70), surface: "mud", subsoil: "clay", soil_depth: 2, trees: 0.01, roughness: 0.4, puddles: 0.3 },
    Biome { name: "moor", glyph: ',', color: Rgb(130, 100, 110), surface: "grass", subsoil: "soil", soil_depth: 1, trees: 0.002, roughness: 1.5, puddles: 0.05 },
    Biome { name: "desert", glyph: '∙', color: Rgb(222, 196, 120), surface: "sand", subsoil: "sand", soil_depth: 4, trees: 0.0, roughness: 1.5, puddles: 0.0 },
    Biome { name: "snow", glyph: '*', color: Rgb(230, 236, 246), surface: "snow", subsoil: "soil", soil_depth: 1, trees: 0.004, roughness: 1.5, puddles: 0.0 },
    Biome { name: "rock", glyph: '▲', color: Rgb(150, 146, 150), surface: "", subsoil: "gravel", soil_depth: 0, trees: 0.0, roughness: 2.5, puddles: 0.0 },
    Biome { name: "ashland", glyph: '`', color: Rgb(110, 104, 100), surface: "ash", subsoil: "ash", soil_depth: 2, trees: 0.0, roughness: 1.2, puddles: 0.0 },
];

pub fn biome(name: &str) -> &'static Biome {
    BIOMES.iter().find(|b| b.name == name).unwrap_or(&BIOMES[0])
}

/// Feature kinds offered on the atlas (blueprints are added from disk).
pub const FEATURES: [&str; 14] =
    ["town", "village", "farm", "keep", "chapel", "crypt", "ruin", "cave", "shrine", "tower", "camp", "mine", "port", "landmark"];

pub fn gen_settings(seed: u32) -> GenSettings {
    let mats = Materials::standard();
    let id = |n: &str| mats.find(n).unwrap_or_else(|| panic!("no material {n}"));
    let biomes = BIOMES
        .iter()
        .map(|b| BiomeDef {
            name: b.name.into(),
            surface: if b.surface.is_empty() { None } else { Some(id(b.surface)) },
            subsoil: id(b.subsoil),
            soil_depth: b.soil_depth,
            trees: b.trees,
            roughness: b.roughness,
            puddles: b.puddles,
        })
        .collect();
    GenSettings {
        water: id("water"),
        bed: id("gravel"),
        trunk: id("wood"),
        leaves: id("leaves"),
        headroom: 10,
        seed,
        biomes,
        materials: mats,
    }
}

/// Furniture and fittings for building, registered in every local scene.
pub fn register_kinds(t: &mut Things) {
    let brown = Rgb(176, 124, 72);
    let stone = Rgb(190, 186, 178);
    for k in [
        ThingKind::new("door", '◘', brown, Layer::Furniture).blocking().door('\''),
        ThingKind::new("table", 'π', brown, Layer::Furniture).surface(),
        ThingKind::new("chair", '╥', brown, Layer::Furniture),
        ThingKind::new("bed", 'Θ', Rgb(190, 70, 64), Layer::Furniture),
        ThingKind::new("chest", '■', Rgb(150, 100, 52), Layer::Furniture).surface(),
        ThingKind::new("bookshelf", '≡', brown, Layer::Furniture).blocking().surface(),
        ThingKind::new("barrel", 'ö', Rgb(150, 104, 56), Layer::Furniture).blocking(),
        ThingKind::new("torch", '¡', Rgb(255, 210, 110), Layer::Furniture).light(Rgb(255, 176, 92), true).flame(Flame::Exposed),
        ThingKind::new("brazier", 'Ω', Rgb(230, 130, 50), Layer::Furniture).blocking().light(Rgb(255, 150, 60), true).flame(Flame::Sheltered),
        ThingKind::new("candle", 'i', Rgb(250, 240, 200), Layer::Item).light(Rgb(170, 130, 70), true).flame(Flame::Exposed),
        ThingKind::new("altar", '╤', stone, Layer::Furniture).blocking().surface(),
        ThingKind::new("pew", '═', brown, Layer::Furniture),
        ThingKind::new("coffin", '▬', Rgb(90, 60, 40), Layer::Furniture).blocking(),
        ThingKind::new("statue", 'Ä', stone, Layer::Furniture).blocking(),
        ThingKind::new("well", 'O', stone, Layer::Furniture).blocking(),
        ThingKind::new("anvil", '¬', Rgb(120, 120, 130), Layer::Furniture).blocking(),
        ThingKind::new("hay bale", '"', Rgb(220, 200, 100), Layer::Furniture),
        ThingKind::new("sign", '¶', brown, Layer::Furniture),
        ThingKind::new("rug", '░', Rgb(150, 50, 50), Layer::Item),
        ThingKind::new("plank bridge", '=', Rgb(150, 110, 70), Layer::Furniture).deck(),
    ] {
        t.add_kind(k);
    }
}
