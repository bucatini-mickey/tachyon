//! Pictures. Everything draws into a [`frame::Frame`] of character cells:
//! the map (`view`), the screens around it (`ui`). How a frame reaches the
//! screen (pure ASCII or not, which colours) is decided by `display`.

pub mod display;
pub mod frame;
pub mod ui;
pub mod view;
