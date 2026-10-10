//! Materials: what cells are made of, and the physical numbers that drive the
//! simulation. Liquids and gases are materials too, so one registry covers
//! stone walls, water, lava and smoke alike.
//!
//! A game registers its own materials; [`Materials::standard`] is a ready-made
//! set (rock, soil, wood, water, lava, smoke, ...) that games can start from.

use crate::color::Rgb;
use std::collections::HashMap;

pub type MatId = u16;

/// Material 0 is always air: "nothing here".
pub const AIR: MatId = 0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatKind {
    Solid,
    /// `viscosity`: spreads sideways once every `viscosity` ticks (1 = water).
    Liquid { viscosity: u8 },
    /// `rise`: fraction that moves up each tick. `decay`: fraction lost per
    /// tick (four times faster under open sky). `opacity`: above this density
    /// the gas blocks sight.
    Gas { rise: f32, decay: f32, opacity: u8 },
}

/// How a wall of this material is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallStyle {
    /// Every cell draws `wall_glyph` (natural rock, soil).
    Solid,
    /// Joined single lines `─│┌┼` (timber, palisades).
    Lines,
    /// Joined double lines `═║╔╬` (masonry).
    DoubleLines,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub kind: MatKind,
    /// Colour of the glyph, and of the background behind it.
    pub fg: Rgb,
    pub bg: Rgb,
    pub wall_glyph: char,
    /// Floors pick one of these per cell, for texture.
    pub floor_glyphs: Vec<char>,
    pub style: WallStyle,
    /// Walls of this material let light and sight through (glass, ice).
    pub transparent: bool,

    /// Thermal conductance between neighbouring cells, per tick. Keep
    /// at or below about 0.12 so the explicit heat step stays stable.
    pub conduct: f32,
    /// Heat capacity relative to air.
    pub heat_cap: f32,

    /// Solid becomes this liquid above the temperature (ice -> water).
    pub melt: Option<(f32, MatId)>,
    /// Liquid becomes this solid below the temperature (water -> ice).
    pub freeze: Option<(f32, MatId)>,
    /// Liquid becomes this gas above the temperature (water -> steam).
    pub boil: Option<(f32, MatId)>,
    /// A liquid that holds itself at this temperature (magma).
    pub self_temp: Option<f32>,

    /// Catches fire above this temperature.
    pub ignite: Option<f32>,
    /// How many ticks a cell of it burns for.
    pub burn_ticks: u16,
    /// What is left after burning (AIR = nothing).
    pub burns_to: MatId,
    /// Gas given off while burning (AIR = none).
    pub burn_gas: MatId,
    /// Chance per tick that a burning cell sets a random flammable neighbour
    /// alight (flames licking across). Heat alone rarely carries a fire along
    /// a wall, since the open air around it soaks up too much. A fire keeps
    /// spreading when about `burn_ticks * spread * (flammable neighbours / 6)`
    /// is above 1, and dies out below it.
    pub spread: f32,

    /// Gives off light of this colour (lava, glowing fungus).
    pub glow: Option<Rgb>,
    /// Blast it takes to break a wall of it (and 1.5x that for a floor of it
    /// hanging over open space). See `blast`.
    pub strength: f32,
    /// Chance per tick that a thin film (1-2 units) of this liquid evaporates.
    pub evaporate: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            name: String::new(),
            kind: MatKind::Solid,
            fg: Rgb(200, 200, 200),
            bg: Rgb(40, 40, 40),
            wall_glyph: '#',
            floor_glyphs: vec!['.'],
            style: WallStyle::Solid,
            transparent: false,
            conduct: 0.03,
            heat_cap: 2.0,
            melt: None,
            freeze: None,
            boil: None,
            self_temp: None,
            ignite: None,
            burn_ticks: 0,
            burns_to: AIR,
            burn_gas: AIR,
            spread: 0.0,
            glow: None,
            strength: 50.0,
            evaporate: 0.0,
        }
    }
}

impl Material {
    pub fn is_liquid(&self) -> bool {
        matches!(self.kind, MatKind::Liquid { .. })
    }
    pub fn viscosity(&self) -> u8 {
        match self.kind {
            MatKind::Liquid { viscosity } => viscosity.max(1),
            _ => 1,
        }
    }
    /// (rise, decay, opacity) for gases.
    pub fn gas_props(&self) -> (f32, f32, u8) {
        match self.kind {
            MatKind::Gas { rise, decay, opacity } => (rise, decay, opacity),
            _ => (0.0, 0.01, 255),
        }
    }
}

/// The material registry. Look materials up by name once at start-up and keep
/// the `MatId`s; lookups by id are a plain array index.
#[derive(Clone, Debug)]
pub struct Materials {
    list: Vec<Material>,
    names: HashMap<String, MatId>,
}

impl Default for Materials {
    fn default() -> Self {
        Self::new()
    }
}

impl Materials {
    /// A registry containing only air.
    pub fn new() -> Materials {
        let mut m = Materials { list: Vec::new(), names: HashMap::new() };
        m.add(Material {
            name: "air".into(),
            fg: Rgb::BLACK,
            bg: Rgb::BLACK,
            wall_glyph: ' ',
            floor_glyphs: vec![' '],
            conduct: 0.06,
            heat_cap: 1.0,
            ..Default::default()
        });
        m
    }

    pub fn add(&mut self, m: Material) -> MatId {
        let id = self.list.len() as MatId;
        self.names.insert(m.name.clone(), id);
        self.list.push(m);
        id
    }

    #[inline(always)]
    pub fn get(&self, id: MatId) -> &Material {
        &self.list[id as usize]
    }

    pub fn get_mut(&mut self, id: MatId) -> &mut Material {
        &mut self.list[id as usize]
    }

    pub fn find(&self, name: &str) -> Option<MatId> {
        self.names.get(name).copied()
    }

    /// Look up a material by name, panicking with a clear message if missing.
    pub fn id(&self, name: &str) -> MatId {
        self.find(name).unwrap_or_else(|| panic!("no material named '{name}'"))
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// A general-purpose starting set. Names: granite, soil, sand, grass,
    /// wood, leaves, stone block, slate, glass, ash, ice, obsidian, water,
    /// lava, steam, smoke, mist; rocks sandstone, limestone, basalt, marble,
    /// chalk; ground cover snow, mud, clay, gravel.
    pub fn standard() -> Materials {
        let mut m = Materials::new();
        let solid = |name: &str, fg: Rgb, bg: Rgb, wall: char, floors: &[char]| Material {
            name: name.into(),
            fg,
            bg,
            wall_glyph: wall,
            floor_glyphs: floors.to_vec(),
            ..Default::default()
        };

        let steam = m.add(Material {
            name: "steam".into(),
            kind: MatKind::Gas { rise: 0.6, decay: 0.03, opacity: 110 },
            fg: Rgb(224, 226, 236),
            bg: Rgb(150, 150, 160),
            ..Default::default()
        });
        let smoke = m.add(Material {
            name: "smoke".into(),
            kind: MatKind::Gas { rise: 0.45, decay: 0.006, opacity: 90 },
            fg: Rgb(120, 118, 116),
            bg: Rgb(70, 68, 66),
            ..Default::default()
        });
        // Fog: hugs the ground (does not rise), lingers, and drifts with the wind.
        m.add(Material {
            name: "mist".into(),
            kind: MatKind::Gas { rise: 0.0, decay: 0.0005, opacity: 70 },
            fg: Rgb(196, 202, 210),
            bg: Rgb(112, 118, 126),
            ..Default::default()
        });
        m.add(Material {
            conduct: 0.03,
            heat_cap: 2.2,
            strength: 400.0,
            ..solid("granite", Rgb(118, 116, 124), Rgb(46, 44, 52), '▓', &['.', '·', '.', ','])
        });
        m.add(Material {
            conduct: 0.025,
            heat_cap: 1.8,
            strength: 60.0,
            ..solid("soil", Rgb(128, 92, 58), Rgb(54, 38, 24), '▒', &['.', ',', '·', '.'])
        });
        m.add(Material {
            conduct: 0.025,
            heat_cap: 1.6,
            strength: 30.0,
            ..solid("sand", Rgb(196, 180, 128), Rgb(84, 74, 46), '▒', &['.', '·', ',', '.'])
        });
        let ash = m.add(Material {
            conduct: 0.02,
            heat_cap: 1.2,
            strength: 10.0,
            ..solid("ash", Rgb(105, 100, 96), Rgb(34, 32, 30), '▒', &[',', '.', '·', '\''])
        });
        m.add(Material {
            ignite: Some(260.0),
            burn_ticks: 25,
            burns_to: ash,
            burn_gas: smoke,
            spread: 0.04,
            conduct: 0.025,
            heat_cap: 1.5,
            strength: 20.0,
            ..solid("grass", Rgb(84, 150, 58), Rgb(26, 46, 18), '"', &[',', '.', '\'', '"', '`', ',', '.'])
        });
        m.add(Material {
            style: WallStyle::Lines,
            ignite: Some(300.0),
            burn_ticks: 400,
            burns_to: ash,
            burn_gas: smoke,
            spread: 0.1,
            conduct: 0.02,
            heat_cap: 1.6,
            strength: 80.0,
            ..solid("wood", Rgb(168, 118, 66), Rgb(62, 40, 22), 'O', &['+'])
        });
        m.add(Material {
            ignite: Some(240.0),
            burn_ticks: 40,
            burns_to: AIR,
            burn_gas: smoke,
            spread: 0.2,
            conduct: 0.03,
            heat_cap: 1.2,
            strength: 5.0,
            ..solid("leaves", Rgb(60, 134, 48), Rgb(18, 44, 16), '♣', &['♣'])
        });
        m.add(Material {
            style: WallStyle::DoubleLines,
            conduct: 0.03,
            heat_cap: 2.2,
            strength: 250.0,
            ..solid("stone block", Rgb(178, 176, 168), Rgb(62, 60, 58), '#', &['+'])
        });
        m.add(Material {
            conduct: 0.03,
            heat_cap: 2.0,
            strength: 200.0,
            ..solid("slate", Rgb(96, 104, 124), Rgb(36, 40, 50), '▓', &['≡'])
        });
        m.add(Material {
            transparent: true,
            conduct: 0.04,
            strength: 15.0,
            ..solid("glass", Rgb(160, 214, 232), Rgb(30, 56, 66), '░', &['.'])
        });
        // More rock, for world building. Strength is for blasts.
        for (name, fg, bg, strength) in [
            ("sandstone", Rgb(206, 160, 110), Rgb(92, 64, 40), 120.0),
            ("limestone", Rgb(206, 200, 176), Rgb(86, 82, 70), 180.0),
            ("basalt", Rgb(92, 92, 100), Rgb(30, 30, 36), 380.0),
            ("marble", Rgb(232, 228, 222), Rgb(110, 106, 104), 260.0),
            ("chalk", Rgb(236, 234, 220), Rgb(130, 128, 116), 60.0),
        ] {
            m.add(Material { conduct: 0.03, heat_cap: 2.1, strength, ..solid(name, fg, bg, '▓', &['.', '·', ',', '.']) });
        }
        // Ground cover.
        m.add(Material { conduct: 0.02, heat_cap: 2.0, strength: 10.0, ..solid("snow", Rgb(236, 240, 250), Rgb(150, 160, 180), '▒', &['.', '·', '\'', '.']) });
        m.add(Material { conduct: 0.03, heat_cap: 2.5, strength: 15.0, ..solid("mud", Rgb(110, 84, 56), Rgb(48, 34, 22), '▒', &['~', ',', '.', ',']) });
        m.add(Material { conduct: 0.03, heat_cap: 2.0, strength: 40.0, ..solid("clay", Rgb(170, 100, 70), Rgb(78, 44, 30), '▒', &['.', ',', '·']) });
        m.add(Material { conduct: 0.03, heat_cap: 1.8, strength: 30.0, ..solid("gravel", Rgb(150, 146, 140), Rgb(60, 58, 56), '▒', &[':', '·', '.', ':']) });
        let ice = m.add(Material {
            transparent: true,
            conduct: 0.06,
            heat_cap: 2.0,
            strength: 40.0,
            ..solid("ice", Rgb(200, 236, 255), Rgb(110, 160, 200), '▓', &['·'])
        });
        let obsidian = m.add(Material {
            conduct: 0.03,
            heat_cap: 2.0,
            strength: 300.0,
            ..solid("obsidian", Rgb(80, 64, 96), Rgb(22, 16, 28), '▓', &['.', '·'])
        });
        let water = m.add(Material {
            name: "water".into(),
            kind: MatKind::Liquid { viscosity: 1 },
            fg: Rgb(70, 130, 235),
            bg: Rgb(12, 44, 130),
            freeze: Some((0.0, ice)),
            boil: Some((100.0, steam)),
            conduct: 0.08,
            heat_cap: 4.0,
            evaporate: 0.002,
            ..Default::default()
        });
        m.add(Material {
            name: "lava".into(),
            kind: MatKind::Liquid { viscosity: 6 },
            fg: Rgb(255, 150, 40),
            bg: Rgb(170, 34, 0),
            freeze: Some((700.0, obsidian)),
            self_temp: Some(1200.0),
            glow: Some(Rgb(255, 110, 30)),
            conduct: 0.08,
            heat_cap: 3.0,
            ..Default::default()
        });
        m.get_mut(ice).melt = Some((0.5, water));
        m
    }
}
