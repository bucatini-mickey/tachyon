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
//! Layering (each layer only knows about the ones above it in this list):
//!
//! ```text
//!  color, geom, rng, noise        maths and utilities
//!  material, tile, world          the cell grid and what cells are made of
//!  things, clock                  objects and creatures in cells; time of day
//!  sim                            physics over the grid (fluid, gas, heat, fire, light)
//!  vessel                         rafts and boats floating on the liquid
//!  movement                       moving a creature through the grid
//!  life                           creatures: bodies and wounds, needs, minds and routines
//!  scene                          world + things + clock + sim + vessels + life, ticked together
//!  projectile, weather            things in flight and explosions; wind and rain
//!  sea, craft, ballistics, sight  the open sea as fields; ships as scenes posed on it;
//!                                 round shot; seeing over the horizon
//!  frame, view                    composing a picture of the world into a cell buffer
//!  term, input, app               the terminal backend and the main loop
//! ```
//!
//! Nothing below `term` touches the terminal, so the simulation runs headless
//! (tests, benchmarks, servers) and a different backend (web canvas, native
//! window, phone) only needs to display a [`frame::Frame`] and feed back
//! [`input::KeyEvent`]s.

pub mod app;
pub mod atlas;
pub mod ballistics;
pub mod blueprint;
pub mod clock;
pub mod color;
pub mod craft;
pub mod frame;
pub mod geom;
pub mod input;
pub mod life;
pub mod material;
pub mod movement;
pub mod noise;
pub mod projectile;
pub mod rng;
pub mod scene;
pub mod sea;
pub mod sight;
pub mod sim;
pub mod term;
pub mod things;
pub mod tile;
pub mod vessel;
pub mod view;
pub mod weather;
pub mod world;
pub mod worldgen;

/// Everything a game usually needs, in one import.
pub mod prelude {
    pub use crate::app::{run, Flow, Game, LoopStats};
    pub use crate::clock::{Clock, Daylight};
    pub use crate::color::Rgb;
    pub use crate::frame::{Cell, Frame};
    pub use crate::geom::{Pos, Rect, DIRS4, DIRS8};
    pub use crate::input::{DirPad, Key, KeyEvent, MouseButton, MouseEvent, MouseKind};
    pub use crate::life::body::{DamageKind, Does, Tissue};
    pub use crate::life::mind::{Control, Routine, Task};
    pub use crate::life::species::{Attack, Species};
    pub use crate::life::{Life, LifeEvent, SpeciesId};
    pub use crate::material::{MatId, MatKind, Material, Materials, WallStyle, AIR};
    pub use crate::movement::{self, Step};
    pub use crate::rng::{hash3, Rng};
    pub use crate::projectile::{Blast, Projectile};
    pub use crate::scene::{Scene, SceneEvent};
    pub use crate::things::{Flame, Food, KindId, Layer, Thing, ThingId, ThingKind, Things};
    pub use crate::tile::Tile;
    pub use crate::vessel::{Afloat, Vessel};
    pub use crate::view::{MapView, Overlay};
    pub use crate::weather::Weather;
    pub use crate::world::{World, FULL, SKY_MAX};
}
