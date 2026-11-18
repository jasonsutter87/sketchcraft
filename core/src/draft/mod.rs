//! Drafting: crisp vector drawings. One paper-space display list (`ir`) feeds the PDF, SVG and
//! DXF writers; `sheet` turns pages and viewports into that list, with hidden-line removal
//! (`hidden`) for elevations, sections and parallel views. See docs/dev/drafting-design.md.

pub mod dxf;
pub mod font;
pub mod hidden;
pub mod ir;
pub mod paper;
pub mod pdf;
pub mod project;
pub mod sheet;
pub mod svg;
