//! Rendering backends. Each consumes the shared [`crate::scene::Scene`] and
//! produces output in its own format.

pub mod skia;
pub mod svg;
