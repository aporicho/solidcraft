//! Original procedural T-slot sections using common European-series nominal dimensions.
//! The catalogue identifies fixed section templates; length and placement are editable.
use solidcraft_geom::{Loop2, Region2, Vec2};

use crate::{DocError, Result};

/// Fixed stock catalogue. A model selects an immutable section; only length and
/// placement are feature parameters. Different supplier dies need distinct catalogue IDs.
pub const MODELS: &[(&str, f64, f64, f64, f64, f64)] =
    &[("2020", 20.0, 6.2, 5.0, 1.8, 0.8), ("3030", 30.0, 8.2, 6.8, 2.0, 1.0), ("4040", 40.0, 8.2, 6.8, 2.0, 1.0)];

pub fn stock_section(model: &str) -> Result<Region2> {
    let entry = MODELS.iter().find(|entry| entry.0 == model).ok_or_else(|| DocError::Invalid(format!("unknown aluminium section model {model}")))?;
    section(entry.1, entry.2, entry.3, entry.4, entry.5)
}

/// Four open T-slots, diagonal webs, and one through bore. The outline is a single
/// closed wire, so the result stays one true solid rather than overlapping bars.
fn section(size: f64, slot: f64, bore: f64, wall: f64, radius: f64) -> Result<Region2> {
    let invalid = |text: &str| DocError::Invalid(format!("aluminium profile: {text}"));
    if [size, slot, bore, wall, radius].iter().any(|v| !v.is_finite())
        || !(10.0..=200.0).contains(&size)
        || slot <= 0.0
        || bore <= 0.0
        || wall < 0.5
        || radius < 0.0
    {
        return Err(invalid("finite positive section dimensions are required (side 10–200 mm, wall at least 0.5 mm)"));
    }
    let h = size / 2.0;
    let opening = slot / 2.0;
    let cavity = h - 2.5 * wall;
    let shoulder = h - 2.0 * wall;
    let root = bore / 2.0 + wall;
    let flat = bore * 0.35;
    if cavity <= opening + 0.3 || shoulder <= root + 0.3 || flat >= opening || radius > wall.min((h - opening) / 2.0) {
        return Err(invalid("slot, bore, wall and radius leave no connected web; reduce the opening, bore, wall or radius"));
    }
    // Traverse the top from left to right, then rotate clockwise around the four sides.
    let top = [
        Vec2::new(-h, h),
        Vec2::new(-opening, h),
        Vec2::new(-opening, h - wall),
        Vec2::new(-cavity, h - wall),
        Vec2::new(-cavity, shoulder),
        Vec2::new(-flat, root),
        Vec2::new(flat, root),
        Vec2::new(cavity, shoulder),
        Vec2::new(cavity, h - wall),
        Vec2::new(opening, h - wall),
        Vec2::new(opening, h),
    ];
    let mut points = Vec::with_capacity(44);
    let mut radii = Vec::with_capacity(44);
    for side in 0..4 {
        for (index, point) in top.iter().enumerate() {
            let mut p = *point;
            for _ in 0..side {
                p = Vec2::new(p.y, -p.x);
            }
            points.push(p);
            radii.push(if index == 0 { radius } else { 0.0 });
        }
    }
    let outer = crate::frame::rounded_polygon(&points, &radii)?.ccw();
    Ok(Region2 { outer, holes: vec![Loop2::circle(Vec2::ZERO, bore / 2.0).reversed()] })
}
