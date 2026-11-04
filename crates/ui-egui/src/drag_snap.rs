//! Snapping while a sketch point or a line is dragged: the dragged point snaps to other points
//! (ends, centres, the origin, projected points), to the middle of a line and onto curves, and
//! lines up horizontally or vertically with other points (a dashed guide). Letting go on a point,
//! a midpoint or a curve adds the matching constraint; lining up adds none. The suppress-snap
//! chord is configurable (default Ctrl/Cmd or Alt/Option; see `prefs::suppress_sketch_snap`).

use std::cell::{Cell, RefCell};

use egui::{FontId, Stroke};
use serde_json::{Value, json};
use solidcraft_engine::geom::Vec2;
use solidcraft_engine::sketch::{ConstraintKind, CurveKind, Sketch};

use crate::SolveApp;
use crate::theme::Tokens;
use crate::viewport::Proj;

/// What the dragged point snapped to.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Another point (its id): coincident on release.
    Point(String),
    /// The middle of a line (its id): midpoint on release.
    Mid(String),
    /// On a curve (its id): point on curve on release.
    On(String),
    /// On a curve at its intersection with a fixed grid line.
    OnGrid(String),
    /// Lined up with other points: no constraint.
    Align,
    /// Fixed grid position: no geometric constraint is added on release.
    Grid,
}

/// A snap: the moving point (its id) and where it goes, the move added to the cursor's, the
/// target, and the dashed guides (sketch coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct Snap {
    pub point: String,
    pub at: Vec2,
    pub offset: Vec2,
    pub target: Target,
    pub guides: Vec<(Vec2, Vec2)>,
}

impl Snap {
    pub fn label(&self) -> &'static str {
        match self.target {
            Target::Point(_) => "Pt",
            Target::Mid(_) => "Mid",
            Target::On(_) => "On",
            Target::OnGrid(_) => "On+Grid",
            Target::Align => "",
            Target::Grid => "Grid",
        }
    }
}

/// Snap distances, in screen pixels.
const POINT_PX: f64 = 8.0;
const CURVE_PX: f64 = 6.0;
const ALIGN_PX: f64 = 6.0;

thread_local! {
    static CURRENT: RefCell<Option<Snap>> = const { RefCell::new(None) };
    static SUPPRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Whether sketch snap is suppressed this frame (set from the viewport via `prefs::suppress_sketch_snap_held`).
pub fn set_suppressed(on: bool) {
    SUPPRESSED.with(|s| s.set(on));
}

pub fn suppressed() -> bool {
    SUPPRESSED.with(Cell::get)
}

pub fn current() -> Option<Snap> {
    CURRENT.with(|c| c.borrow().clone())
}

pub fn clear() {
    CURRENT.with(|c| *c.borrow_mut() = None);
}

fn keep(s: Option<Snap>) -> Option<Snap> {
    CURRENT.with(|c| *c.borrow_mut() = s.clone());
    s
}

/// The best snap for points moving to `moving` (point index, position after the cursor's move),
/// `mm_px` millimetres per pixel. Points in `exclude` (the ones moving with the drag) are not
/// targets, nor are curves using them.
pub fn find(sk: &Sketch, moving: &[(usize, Vec2)], exclude: &[usize], mm_px: f64) -> Option<Snap> {
    find_with_grid(sk, moving, exclude, mm_px, 7.0, None)
}

/// The drawing preferences also apply while dragging: geometric targets first, then the
/// fixed grid on any free axes. `None` keeps the original unquantized drag in empty space.
pub fn find_with_grid(sk: &Sketch, moving: &[(usize, Vec2)], exclude: &[usize], mm_px: f64, radius_px: f64, grid_step: Option<f64>) -> Option<Snap> {
    if suppressed() || moving.is_empty() || !(mm_px > 0.0 && mm_px.is_finite()) {
        return keep(None);
    }
    let radius_px = if radius_px.is_finite() { radius_px.clamp(2.0, 40.0) } else { 7.0 };
    let (point_px, curve_px, align_px) = if radius_px == 7.0 { (POINT_PX, CURVE_PX, ALIGN_PX) } else { (radius_px, radius_px * 0.8, radius_px) };
    let grid_step = grid_step.filter(|step| step.is_finite() && *step > 0.0 && *step <= 10000.0);
    let id = |i: usize| sk.points.get(i).map(|p| p.id.clone()).unwrap_or_default();
    let visible = solidcraft_engine::view::SketchPointVisibility::new(sk);
    let targets: Vec<(usize, Vec2)> =
        sk.points.iter().enumerate().filter(|(i, p)| !exclude.contains(i) && visible.takes(p)).map(|(i, p)| (i, p.pos)).collect();
    let free_curves: Vec<usize> =
        (0..sk.curves.len()).filter(|&c| sk.curves.get(c).is_some_and(|cu| !exclude.iter().any(|&p| cu.kind.uses(p)))).collect();
    let mut best: Option<(f64, Snap)> = None;
    let offer = |best: &mut Option<(f64, Snap)>, d: f64, s: Snap| {
        if best.as_ref().is_none_or(|b| d < b.0) {
            *best = Some((d, s));
        }
    };
    // Points, then midpoints, then curves.
    for &(mi, p) in moving {
        for &(ti, t) in &targets {
            let d = p.dist(t);
            if d < point_px * mm_px {
                offer(&mut best, d, Snap { point: id(mi), at: t, offset: t - p, target: Target::Point(id(ti)), guides: Vec::new() });
            }
        }
    }
    if best.is_none() {
        for &(mi, p) in moving {
            for &c in &free_curves {
                let Some(cu) = sk.curves.get(c) else { continue };
                let CurveKind::Line { a, b } = cu.kind else { continue };
                let (Some(a), Some(b)) = (sk.point(a), sk.point(b)) else { continue };
                let m = (a + b) * 0.5;
                let d = p.dist(m);
                if d < point_px * mm_px {
                    offer(&mut best, d, Snap { point: id(mi), at: m, offset: m - p, target: Target::Mid(cu.id.clone()), guides: Vec::new() });
                }
            }
        }
    }
    if best.is_none() {
        for &(mi, p) in moving {
            for &c in &free_curves {
                let (Some(cu), Some(sh)) = (sk.curves.get(c), sk.shape(c)) else { continue };
                let d = sh.dist(p);
                if d < curve_px * mm_px {
                    let q = sh.project(p);
                    let grid = grid_step.and_then(|step| crate::sketch_tools::snap_on_shape_grid(&sh, q, step, cu.infinite));
                    let (q, target) = grid.map_or_else(|| (q, Target::On(cu.id.clone())), |q| (q, Target::OnGrid(cu.id.clone())));
                    offer(&mut best, d, Snap { point: id(mi), at: q, offset: q - p, target, guides: Vec::new() });
                }
            }
        }
    }
    if let Some((_, s)) = best {
        return keep(Some(s));
    }
    // Lining up: the nearest horizontal and the nearest vertical alignment, each on its own axis.
    let tol = align_px * mm_px;
    let (mut hx, mut vy): (Option<(f64, usize, Vec2, Vec2)>, Option<(f64, usize, Vec2, Vec2)>) = (None, None);
    for &(mi, p) in moving {
        for &(_, t) in &targets {
            let (dy, dx) = ((p.y - t.y).abs(), (p.x - t.x).abs());
            if dy < tol && dx > tol && hx.is_none_or(|h| dy < h.0) {
                hx = Some((dy, mi, p, t));
            }
            if dx < tol && dy > tol && vy.is_none_or(|v| dx < v.0) {
                vy = Some((dx, mi, p, t));
            }
        }
    }
    let mut offset = Vec2::ZERO;
    let mut guides = Vec::new();
    let mut point = None;
    if let Some((_, mi, p, t)) = hx {
        offset.y = t.y - p.y;
        point = Some(mi);
        guides.push((t, Vec2::new(p.x, t.y)));
    }
    if let Some((_, mi, p, t)) = vy {
        offset.x = t.x - p.x;
        point = point.or(Some(mi));
        guides.push((t, Vec2::new(t.x, p.y)));
    }
    // The guides end at the snapped point (both alignments may move it).
    let anchor = point.and_then(|mi| moving.iter().find(|m| m.0 == mi)).or_else(|| grid_step.and_then(|_| moving.first()));
    let Some(&(mi, p)) = anchor else { return keep(None) };
    if let Some(step) = grid_step {
        if vy.is_none() {
            offset.x = crate::sketch_tools::snap_grid_value(p.x, step) - p.x;
        }
        if hx.is_none() {
            offset.y = crate::sketch_tools::snap_grid_value(p.y, step) - p.y;
        }
    }
    let at = p + offset;
    for g in &mut guides {
        g.1 = if (g.0.y - g.1.y).abs() < 1e-12 { Vec2::new(at.x, g.0.y) } else { Vec2::new(g.0.x, at.y) };
    }
    let target = if point.is_some() { Target::Align } else { Target::Grid };
    keep(Some(Snap { point: id(mi), at, offset, target, guides }))
}

/// Points the drag of `entities` (point or curve ids: the one grabbed and those dragged with it)
/// moves, and the points that move with them.
pub fn moving_points<'a>(sk: &Sketch, entities: impl IntoIterator<Item = &'a str>) -> (Vec<usize>, Vec<usize>) {
    let mut own: Vec<usize> = entities
        .into_iter()
        .flat_map(|entity| {
            if let Some(p) = sk.point_index(entity) {
                vec![p]
            } else {
                match sk.curve_index(entity).and_then(|c| sk.curves.get(c)) {
                    // Only lines slide whole; a circle or arc grabbed on its edge changes its radius.
                    Some(c) if matches!(c.kind, CurveKind::Line { .. }) => c.kind.point_ids(),
                    _ => Vec::new(),
                }
            }
        })
        .collect();
    // A point held at the middle of a line (a centre rectangle's centre on its diagonal) goes
    // where the line's ends go, and dragging it takes the line along. So it moves, and snaps,
    // with a line whose ends both move; and when it is dragged, its line and the shape round it
    // are no targets for it: they held it at their own middle instead of lining it up (#10086).
    let middles: Vec<(usize, usize, usize)> = sk
        .constraints
        .iter()
        .filter(|c| !c.driven)
        .filter_map(|c| match c.kind {
            ConstraintKind::Midpoint { p, l } => match sk.curves.get(l).map(|c| &c.kind) {
                Some(CurveKind::Line { a, b }) => Some((p, *a, *b)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    for &(p, a, b) in &middles {
        if own.contains(&a) && own.contains(&b) && !own.contains(&p) {
            own.push(p);
        }
    }
    own.sort_unstable();
    own.dedup();
    let mut moved = own.clone();
    moved.extend(middles.iter().filter(|(p, ..)| own.contains(p)).flat_map(|&(_, a, b)| [a, b]));
    let mut ex = moved.clone();
    for c in &sk.curves {
        if moved.iter().any(|&p| c.kind.uses(p)) {
            ex.extend(c.kind.point_ids());
        }
    }
    ex.sort_unstable();
    ex.dedup();
    (own, ex)
}

/// The drag let go: the constraint of the snap it ended on (none for lining up), as
/// `sketch.drag_end` takes it.
pub fn release() -> Option<Value> {
    let s = current()?;
    clear();
    let (cmd, params) = match &s.target {
        Target::Point(t) => ("sketch.constraint.coincident", json!({"a": s.point, "b": t})),
        Target::Mid(l) => ("sketch.constraint.midpoint", json!({"point": s.point, "line": l})),
        Target::On(c) | Target::OnGrid(c) => ("sketch.constraint.coincident", json!({"a": s.point, "b": c})),
        Target::Align | Target::Grid => return None,
    };
    Some(json!({"command": cmd, "params": params}))
}

/// The snap of the drag in progress: its dashed guides, a ring on the target and its label.
pub fn show(app: &SolveApp, painter: &egui::Painter, proj: &Proj) {
    if app.viewport.point_drag.is_none() {
        return;
    }
    let Some(s) = current() else { return };
    let st = app.session.world_state();
    let Some(ss) = app.session.active_sketch.and_then(|i| st.sketch(i)) else { return };
    let to = |p: Vec2| proj.to_screen(ss.plane.to_world(p));
    let tk = Tokens::get();
    for (a, b) in &s.guides {
        if let (Some(a), Some(b)) = (to(*a), to(*b)) {
            painter.extend(crate::viewport::dashed_line(&[a, b], painter.clip_rect(), Stroke::new(1.0, tk.rubber_band), 5.0, 4.0));
        }
    }
    let Some(sp) = to(s.at) else { return };
    painter.circle_stroke(sp, 6.0, Stroke::new(1.5, tk.accent));
    let label = if matches!(s.target, Target::Grid | Target::OnGrid(_)) {
        let key = if matches!(s.target, Target::OnGrid(_)) { "sketch.snap_on_grid_at" } else { "sketch.snap_grid_at" };
        crate::i18n::format(key, &[("x", &format!("{:.3}", s.at.x)), ("y", &format!("{:.3}", s.at.y))])
    } else {
        s.label().to_owned()
    };
    if !label.is_empty() {
        let galley = painter.layout_no_wrap(label, FontId::proportional(tk.font_micro), tk.text);
        let r = egui::Rect::from_min_size(sp + egui::vec2(tk.space_12, -tk.space_24), galley.size() + egui::vec2(tk.space_8, tk.space_4));
        painter.rect_filled(r, tk.radius_sm.saturating_sub(1), tk.accent_soft);
        painter.galley(r.min + egui::vec2(tk.space_4, tk.space_4 * 0.5), galley, tk.text);
    }
    crate::scenario::publish_count("drag_guides", s.guides.len() as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhanced_drag_grid_fills_free_axes_and_keeps_geometry_priority() {
        let s = sk();
        let (own, exclude) = moving_points(&s, ["l2.start"]);
        let at = Vec2::new(37.27, 22.81);
        let moving = [(own[0], at)];
        assert!(find(&s, &moving, &exclude, 0.1).is_none(), "the original drag stays free in empty space");
        let grid = find_with_grid(&s, &moving, &exclude, 0.1, 12.0, Some(1.0)).unwrap();
        assert_eq!(grid.target, Target::Grid);
        assert!(grid.at.dist(Vec2::new(37.0, 23.0)) < 1e-10);
        assert!(release().is_none(), "a grid snap must not create a constraint");
        for (cursor, expected, target) in [
            (Vec2::new(10.3, 0.4), Vec2::new(10.0, 0.0), "Pt"),
            (Vec2::new(5.2, 0.3), Vec2::new(5.0, 0.0), "Mid"),
            (Vec2::new(7.5, 0.3), Vec2::new(9.0, 0.0), "On+Grid"),
            (Vec2::new(40.2, 0.35), Vec2::new(39.0, 0.0), ""),
        ] {
            let snap = find_with_grid(&s, &[(own[0], cursor)], &exclude, 0.1, 7.0, Some(3.0)).unwrap();
            assert!(snap.at.dist(expected) < 1e-10, "{snap:?}");
            assert_eq!(snap.label(), target);
        }
        clear();
    }

    #[test]
    fn enhanced_drag_range_and_suppression_apply_to_grid_and_geometry() {
        let s = sk();
        let (own, exclude) = moving_points(&s, ["l2.start"]);
        let moving = [(own[0], Vec2::new(10.8, 0.8))];
        assert!(find_with_grid(&s, &moving, &exclude, 0.1, 4.0, None).is_none());
        let snap = find_with_grid(&s, &moving, &exclude, 0.1, 18.0, Some(3.0)).unwrap();
        assert!(matches!(snap.target, Target::Point(_)));
        set_suppressed(true);
        let suppressed = find_with_grid(&s, &moving, &exclude, 0.1, 18.0, Some(3.0));
        set_suppressed(false);
        assert!(suppressed.is_none());
        assert!(current().is_none());
    }

    #[test]
    fn enhanced_on_grid_stays_on_offset_lines_diagonals_and_bounded_arcs() {
        use crate::sketch_tools::snap_on_shape_grid;
        use solidcraft_engine::sketch::Shape;
        let vertical = Shape::Line { a: Vec2::new(3.0, -20.0), b: Vec2::new(3.0, 70.0) };
        assert_eq!(snap_on_shape_grid(&vertical, Vec2::new(3.0, 26.3), 10.0, false), Some(Vec2::new(3.0, 30.0)));
        let horizontal = Shape::Line { a: Vec2::new(-20.0, 3.0), b: Vec2::new(70.0, 3.0) };
        assert_eq!(snap_on_shape_grid(&horizontal, Vec2::new(26.3, 3.0), 10.0, false), Some(Vec2::new(30.0, 3.0)));
        let diagonal = Shape::Line { a: Vec2::new(0.0, 1.0), b: Vec2::new(40.0, 21.0) };
        let snapped = snap_on_shape_grid(&diagonal, Vec2::new(11.3, 6.65), 5.0, false).unwrap();
        assert!(diagonal.dist(snapped) < 1e-10, "rounding both coordinates would leave the line: {snapped:?}");
        assert!((snapped.x / 5.0 - (snapped.x / 5.0).round()).abs() < 1e-10 || (snapped.y / 5.0 - (snapped.y / 5.0).round()).abs() < 1e-10);
        let arc = Shape::Round { c: Vec2::ZERO, r: 10.0, start: 0.0, sweep: std::f64::consts::FRAC_PI_2 };
        let snapped = snap_on_shape_grid(&arc, Vec2::new(6.0, 8.0), 5.0, false).unwrap();
        assert!(arc.dist(snapped) < 1e-10 && snapped.x >= 0.0 && snapped.y >= 0.0);
        let short = Shape::Line { a: Vec2::new(3.0, 21.0), b: Vec2::new(3.0, 24.0) };
        assert!(snap_on_shape_grid(&short, Vec2::new(3.0, 23.0), 10.0, false).is_none(), "no extending a finite line to catch the grid");
        assert!(snap_on_shape_grid(&short, Vec2::new(3.0, 23.0), 10.0, true).is_some());
        assert!(snap_on_shape_grid(&vertical, Vec2::new(3.0, 23.0), f64::NAN, false).is_none());
    }

    fn sk() -> Sketch {
        let mut s = Sketch::new();
        let _ = s.add_line(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), None, None, Some("l1"));
        let _ = s.add_line(Vec2::new(20.0, 5.0), Vec2::new(30.0, 15.0), None, None, Some("l2"));
        s
    }

    fn drag(s: &Sketch, entity: &str, p: Vec2) -> Option<Snap> {
        let (own, ex) = moving_points(s, [entity]);
        find(s, &[(own[0], p)], &ex, 0.1)
    }

    /// A centre rectangle 30 × 20 round `center`, as Center Rectangle draws it: the centre `p5`
    /// held at the middle of the diagonal `l5`.
    fn centre_rectangle(center: [f64; 2]) -> Sketch {
        let mut s = solidcraft_engine::Session::default();
        let _ = s.execute("sketch.create", &json!({"plane": "XY"}));
        let corner = [center[0] + 15.0, center[1] + 10.0];
        let _ = s.execute("sketch.rectangle.center", &json!({"center": center, "corner": corner}));
        s.active_sketch.and_then(|id| s.doc.sketch(id).ok()).cloned().unwrap_or_else(Sketch::new)
    }

    /// #10086: dragged by its centre, a centre rectangle comes along with it, and the centre
    /// lines up with the origin (the X axis here), as a circle's centre does. Its own diagonal,
    /// moving with it, held it at the diagonal's middle instead. Each frame of a drag snaps
    /// against the shape where the last frame left it: here just above the axis.
    #[test]
    fn a_centre_rectangles_centre_lines_up_when_dragged() {
        let s = centre_rectangle([30.0, 0.5]);
        let snap = drag(&s, "p5", Vec2::new(30.0, 0.3)).expect("a snap");
        assert_eq!(snap.target, Target::Align, "{snap:?}");
        assert_eq!(snap.at, Vec2::new(30.0, 0.0));
    }

    /// #10086: the whole rectangle dragged (its four sides selected) moves its centre too, and the
    /// centre lines up as its corners do.
    #[test]
    fn a_dragged_centre_rectangle_moves_and_lines_up_its_centre() {
        let s = centre_rectangle([30.0, 5.0]);
        let centre = s.resolve_point("p5").unwrap_or(0);
        let (own, ex) = moving_points(&s, ["l1", "l2", "l3", "l4"]);
        assert!(own.contains(&centre) && ex.contains(&centre), "{own:?} {ex:?}");
        let d = Vec2::new(0.0, -4.7);
        let moving: Vec<(usize, Vec2)> = own.iter().filter_map(|&p| s.point(p).map(|x| (p, x + d))).collect();
        let snap = find(&s, &moving, &ex, 0.1).expect("a snap");
        assert_eq!((snap.point.as_str(), &snap.target), ("p5", &Target::Align), "{snap:?}");
        assert_eq!(snap.at, Vec2::new(30.0, 0.0));
    }

    #[test]
    fn a_dragged_end_snaps_to_points_midpoints_and_curves() {
        let s = sk();
        let end = s.resolve_point("l2.start").map(|i| s.points[i].id.clone()).unwrap_or_default();
        let at = drag(&s, &end, Vec2::new(10.3, 0.4)).expect("point");
        assert_eq!(at.target, Target::Point(s.points[s.resolve_point("l1.end").unwrap_or(0)].id.clone()));
        assert_eq!(at.at, Vec2::new(10.0, 0.0));
        let mid = drag(&s, &end, Vec2::new(5.2, 0.3)).expect("mid");
        assert_eq!(mid.target, Target::Mid("l1".into()));
        let on = drag(&s, &end, Vec2::new(7.5, 0.3)).expect("on");
        assert_eq!(on.target, Target::On("l1".into()));
        assert!(on.at.y.abs() < 1e-12);
    }

    #[test]
    fn lining_up_moves_the_point_but_names_no_constraint() {
        let s = sk();
        let end = s.points[s.resolve_point("l2.end").unwrap_or(0)].id.clone();
        // Level with l1's end (y 0), far from it in x.
        let a = drag(&s, &end, Vec2::new(40.0, 0.35)).expect("aligned");
        assert_eq!(a.target, Target::Align);
        assert_eq!(a.at.y, 0.0);
        assert_eq!(a.guides.len(), 1);
        assert!(drag(&s, &end, Vec2::new(40.0, 3.0)).is_none());
        set_suppressed(true);
        assert!(drag(&s, &end, Vec2::new(10.1, 0.1)).is_none());
        set_suppressed(false);
    }
}
