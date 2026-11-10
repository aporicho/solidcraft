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

/// Semantic interfaces of a straight stock member, independent of mesh face numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, solidcraft_derive::Walk)]
#[serde(rename_all = "snake_case")]
pub enum Port {
    Start,
    End,
    XPositive,
    XNegative,
    YPositive,
    YNegative,
}

impl Port {
    pub const ALL: [Self; 6] = [Self::Start, Self::End, Self::XPositive, Self::XNegative, Self::YPositive, Self::YNegative];
    pub fn key(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::XPositive => "x_positive",
            Self::XNegative => "x_negative",
            Self::YPositive => "y_positive",
            Self::YNegative => "y_negative",
        }
    }
    pub fn is_end(self) -> bool {
        matches!(self, Self::Start | Self::End)
    }
}

/// A member attached to another member's end or nominal outer side plane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, solidcraft_derive::Walk)]
pub struct Attachment {
    pub body: crate::BodyName,
    pub port: Port,
    #[walk(unitless = "Position (0 to 1)")]
    pub position: crate::Expr,
    #[walk(length = "Gap")]
    pub gap: crate::Expr,
    #[serde(default)]
    pub path: Vec<u64>,
    #[serde(default)]
    pub author_path: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum: Option<ConnectorDatum>,
}

/// Frozen CAD-local installation frame. Metadata changes never silently move saved members.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, solidcraft_derive::Walk)]
pub struct ConnectorDatum {
    pub model: String,
    pub port: String,
    pub cad_sha256: String,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub reference: [f64; 3],
    pub size: f64,
    pub slot: u32,
}

impl ConnectorDatum {
    pub fn frame(&self) -> Result<MemberFrame> {
        use solidcraft_geom::Vec3;
        let vector = |v: [f64; 3]| Vec3::new(v[0], v[1], v[2]);
        let axis = vector(self.normal);
        let x = vector(self.reference);
        if self.origin.iter().chain(&self.normal).chain(&self.reference).any(|v| !v.is_finite() || v.abs() > 10_000_000.0)
            || !self.size.is_finite()
            || !(10.0..=200.0).contains(&self.size)
            || (axis.len() - 1.0).abs() > 1e-6
            || (x.len() - 1.0).abs() > 1e-6
            || axis.dot(x).abs() > 1e-6
            || self.model.len() > 256
            || self.port.is_empty()
            || self.port.len() > 128
            || self.cad_sha256.len() > 128
        {
            return Err(DocError::Invalid("invalid connector installation datum".into()));
        }
        Ok(MemberFrame {
            model: format!("{0:.0}{0:.0}", self.size),
            origin: vector(self.origin),
            x,
            y: axis.cross(x),
            axis,
            length: 0.0,
            size: self.size,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MemberFrame {
    pub model: String,
    pub origin: solidcraft_geom::Vec3,
    pub x: solidcraft_geom::Vec3,
    pub y: solidcraft_geom::Vec3,
    pub axis: solidcraft_geom::Vec3,
    pub length: f64,
    pub size: f64,
}

impl MemberFrame {
    pub fn transformed(mut self, m: &crate::Mat) -> Self {
        self.origin = crate::apply_point(m, self.origin);
        self.x = crate::apply_vector(m, self.x);
        self.y = crate::apply_vector(m, self.y);
        self.axis = crate::apply_vector(m, self.axis);
        self
    }
    /// Centre, outward normal, and section X direction at a named interface.
    pub fn port(&self, port: Port, position: f64, gap: f64) -> (solidcraft_geom::Vec3, solidcraft_geom::Vec3, solidcraft_geom::Vec3) {
        use Port::*;
        let middle = self.origin + self.axis * (self.length * position);
        let (point, normal, reference) = match port {
            Start => (self.origin, -self.axis, self.x),
            End => (self.origin + self.axis * self.length, self.axis, self.x),
            XPositive => (middle + self.x * (self.size / 2.0), self.x, self.y),
            XNegative => (middle - self.x * (self.size / 2.0), -self.x, self.y),
            YPositive => (middle + self.y * (self.size / 2.0), self.y, self.x),
            YNegative => (middle - self.y * (self.size / 2.0), -self.y, self.x),
        };
        (point + normal * gap, normal, reference)
    }
}

/// Exact extrusion-cap provenance carries the original section axes through rigid placements.
/// No display tessellation, face indices or longest-axis guessing is used.
pub fn member_frame(doc: &crate::Document, body: &crate::ModelBody) -> Result<MemberFrame> {
    let model = match doc.feature(body.feature).map(|f| &f.kind) {
        Some(crate::FeatureKind::AluminiumProfile { model, .. }) => model,
        Some(crate::FeatureKind::FrameMember { profile: crate::frame::MemberProfile::Stock { model }, .. }) => model,
        _ => return Err(DocError::Invalid("select an unmodified stock aluminium member".into())),
    };
    let size = MODELS.iter().find(|m| m.0 == model).map(|m| m.1).ok_or_else(|| DocError::Invalid("unknown stock model".into()))?;
    let sources = body.body.face_sources().ok_or_else(|| DocError::Invalid("member extrusion interfaces are unavailable".into()))?;
    let cap = |high| {
        sources.iter().flatten().find_map(|source| match source {
            solidcraft_kernel::FaceSource::ExtrudeCap { high: h, plane, .. } if *h == high => Some(*plane),
            _ => None,
        })
    };
    let lo = cap(false).ok_or_else(|| DocError::Invalid("member start interface is unavailable".into()))?;
    let hi = cap(true).ok_or_else(|| DocError::Invalid("member end interface is unavailable".into()))?;
    let direction = hi.origin - lo.origin;
    let axis = direction.normalized().ok_or_else(|| DocError::Invalid("member has no length".into()))?;
    Ok(MemberFrame { model: model.clone(), origin: lo.origin, x: hi.x, y: hi.y, axis, length: direction.len(), size })
}

pub fn attachment_frame(doc: &crate::Document, st: &crate::ModelState, consumer: u64, attachment: &Attachment) -> Result<MemberFrame> {
    let body = st.body(&attachment.body).ok_or_else(|| DocError::Invalid(format!("attached member `{}` is unavailable", attachment.body)))?;
    let placement = |component, path: &[u64]| -> Result<crate::Mat> {
        if component == 0 && path.is_empty() {
            return Ok(crate::IDENTITY);
        }
        doc.placements()
            .into_iter()
            .find(|(id, p, _)| *id == component && p == path)
            .map(|(_, _, matrix)| matrix)
            .ok_or_else(|| DocError::Invalid("the attached member placement no longer exists".into()))
    };
    let parent = placement(body.component, &attachment.path)?;
    let parent = doc.model_body_offset(body).map_or(parent, |offset| crate::mat_mul(&parent, offset));
    let author = placement(consumer, &attachment.author_path)?;
    let inverse = crate::mat_inverse(&author).ok_or_else(|| DocError::Invalid("invalid attachment author frame".into()))?;
    let frame = match &attachment.datum {
        Some(datum) => datum.frame()?,
        None => member_frame(doc, body)?,
    };
    Ok(frame.transformed(&crate::mat_mul(&inverse, &parent)))
}
