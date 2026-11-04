//! Effects and Patterns: hatching lines, grids, puzzles, circle fills, cast shadows and random copies.
//!
//! Pure geometry over page-space outlines in millimetres, y up. The
//! application turns the results into commands
//! (`apps/tracedraw/src/signcut_fx.rs`).
//!
//! - [`hatch`]: parallel, zigzag and crossed lines inside closed outlines.
//! - [`grid`]: grids of lines or cells, and lines dividing a box.
//! - [`puzzle`]: jigsaw cut lines or pieces.
//! - [`circles`]: circles that fill a shape without overlapping.
//! - [`shadow`]: the transform of a flat cast shadow.
//! - [`scatter`]: random places for copies inside a container.
//! - [`region`]: the inside of closed outlines and its queries.
//! - [`rng`]: the seeded random numbers the random features share.

pub mod circles;
pub mod grid;
pub mod hatch;
pub mod puzzle;
pub mod region;
pub mod rng;
pub mod scatter;
pub mod shadow;

pub use circles::{circle_fill, CircleSpec, Disc};
pub use grid::{across_lines, grid_cells, grid_lines, GridSpec};
pub use hatch::{hatch, polyline_path, HatchSpec, HatchStyle};
pub use puzzle::{puzzle, PuzzleSpec, TabShape};
pub use region::Region;
pub use rng::Rng;
pub use scatter::{scatter, ScatterSpec};
pub use shadow::{shadow_outline, shadow_transform, ShadowKind, ShadowSpec};
