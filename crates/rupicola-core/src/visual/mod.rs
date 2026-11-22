//! Offline image and UI-tree measurements for visual verification. Nothing here launches an
//! application: it reads PNG files, frame sequences and a JSON tree that a toolkit-specific
//! helper produced.

mod audit;
mod color;
mod diff;
mod frames;
mod image;
mod matrix;
mod palette;
mod sheet;

pub use audit::{AUDIT_RULES, AuditFinding, Node, Tree, WindowSize, audit};
pub use color::{InvalidColor, Rgb};
pub use diff::{DEFAULT_DIFF_THRESHOLD, Diff, diff_images};
pub use frames::{Frames, analyze_frames, analyze_images, list_pngs};
pub use image::{Image, RGBA_CHANNELS, load, save};
pub use matrix::{MatrixEntry, MatrixSpec, matrix_plan, matrix_run};
pub use palette::{Palette, Roles, Swatch, palette};
pub use sheet::sheet;

use image::{ALPHA, CHANNEL_MAX, RGB_CHANNELS};
