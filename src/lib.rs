//! # Tachyon
//!
//! A cell-simulation engine for ASCII games in the terminal, in the spirit of
//! Dwarf Fortress: the world is a 3D grid of one-metre cells, viewed one
//! z-level at a time, and everything in it (terrain, liquids, gases, heat,
//! light, fire) is modelled per cell.
//!
//! The engine is game-agnostic. A game supplies its materials, its kinds of
//! thing, a map and its own rules, and drives the engine through [`app::Game`].
//!
//! Layers, each folder only using the ones above it:
//!
//! ```text
//!  base/     color geom rng noise                 maths and utilities
//!  grid/     material tile world things clock     the cell world and what is in it
//!            movement vessel scene                moving through it; rafts; the whole ticked together
//!  sim/      fluid gas heat light flames          physics over awake chunks
//!            weather projectile                   wind and rain; things in flight and explosions
//!  life/     body needs mind ai path species      creatures
//!  sea/      field craft ballistics sight         open water as fields; ships posed on it; shot; long sight
//!  make/     atlas blueprint worldgen lang        authored world maps and structures; generated names
//!  draw/     frame view ui display                cell pictures of the world and its screens; how they reach the screen
//!  backend/  term input app                       the terminal, input events and the main loop
//! ```
//!
//! Every module is also re-exported at the crate root under its own name
//! (`tachyon::world`, `tachyon::craft`, ...), so games need not know the folders.
//!
//! Only `backend::term` touches the terminal, so the simulation runs headless
//! (tests, benchmarks, servers) and another backend (web canvas, native
//! window, phone) only needs to display a [`frame::Frame`] and feed back
//! [`input::KeyEvent`]s.

pub mod backend;
pub mod base;
pub mod draw;
pub mod grid;
pub mod life;
pub mod make;
pub mod sea;
pub mod sim;

pub use backend::{app, input, term};
pub use base::{color, geom, noise, rng};
pub use draw::{display, frame, ui, view};
pub use grid::{clock, material, movement, scene, things, tile, vessel, world};
pub use make::{atlas, blueprint, lang, worldgen};
pub use sea::{ballistics, craft, sight};
pub use sim::{projectile, weather};

/// Everything a game usually needs, in one import.
pub mod prelude {
    pub use crate::app::{run, Flow, Game, LoopStats};
    pub use crate::clock::{Clock, Daylight};
    pub use crate::color::Rgb;
    pub use crate::display::{Charset, Display, Scheme};
    pub use crate::frame::{Cell, Frame};
    pub use crate::geom::{Pos, Rect, DIRS4, DIRS8};
    pub use crate::input::{DirPad, Key, KeyEvent, MouseButton, MouseEvent, MouseKind};
    pub use crate::life::body::{DamageKind, Does, Tissue};
    pub use crate::life::mind::{Control, Routine, Task};
    pub use crate::life::species::{Attack, Species};
    pub use crate::life::{Life, LifeEvent, SpeciesId};
    pub use crate::material::{MatId, MatKind, Material, Materials, WallStyle, AIR};
    pub use crate::movement::{self, Step};
    pub use crate::projectile::{Blast, Projectile};
    pub use crate::rng::{hash3, Rng};
    pub use crate::scene::{Scene, SceneEvent};
    pub use crate::things::{Flame, Food, KindId, Layer, Thing, ThingId, ThingKind, Things};
    pub use crate::tile::Tile;
    pub use crate::vessel::{Afloat, Vessel};
    pub use crate::view::{MapView, Overlay};
    pub use crate::weather::Weather;
    pub use crate::world::{World, FULL, SKY_MAX};
}
