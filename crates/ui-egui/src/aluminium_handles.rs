//! Direct model interfaces: extend a stock member from a face or place a connector at an end.
#[path = "connector_handles.rs"]
mod connectors;

use crate::{SolveApp, dialogs::Kind, theme::Tokens, viewport::Proj};
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2, vec2};
use solidcraft_engine::{Sel, aluminium_ports, doc::aluminium::Port};

/// A picked installation interface, separate from CAD face/edge selection.
#[derive(Clone, Debug)]
pub(crate) struct Interface {
    pub body: String,
    pub port: Port,
    pub kind: &'static str,
    pub position: f64,
    pub point: solidcraft_engine::geom::Vec3,
    pub strip: f64,
}

fn selected(app: &SolveApp) -> Option<(Interface, aluminium_ports::MemberInfo)> {
    if app.dialog.is_some() || app.tool.is_some() || app.session.active_sketch.is_some() || app.applying.is_some() || app.ui.tab != "ALUMINIUM" {
        return None;
    }
    let [Sel::Body { name }] = app.session.selection.as_slice() else { return None };
    let mut interface = app.viewport.aluminium_interface.as_ref().filter(|i| &i.body == name)?.clone();
    if !app.body_visible(name) {
        return None;
    }
    let info = aluminium_ports::member(&app.session, name).ok()?;
    let (at, _, _) = info.frame.port(interface.port, interface.position, 0.0);
    let across = if matches!(interface.port, Port::XPositive | Port::XNegative) { info.frame.y } else { info.frame.x };
    interface.point = if interface.port.is_end() { at } else { at + across * interface.strip };
    Some((interface, info))
}

fn action_rect(interface: &Interface, info: &aluminium_ports::MemberInfo, proj: &Proj) -> Option<Rect> {
    let anchor = proj.to_screen(interface.point)?;
    if !proj.rect.shrink(20.0).contains(anchor) {
        return None;
    }
    let size = vec2(158.0, 106.0);
    // Keep the action card outside the selected member, so it cannot intercept the
    // next click on another strip or slot of that same member.
    let mut left = anchor.x;
    let mut right = anchor.x;
    let half = info.frame.size / 2.0;
    for z in [0.0, info.frame.length] {
        for x in [-half, half] {
            for y in [-half, half] {
                if let Some(p) = proj.to_screen(info.frame.origin + info.frame.axis * z + info.frame.x * x + info.frame.y * y) {
                    left = left.min(p.x);
                    right = right.max(p.x);
                }
            }
        }
    }
    let x = if right + 16.0 + size.x < proj.rect.right() { right + 16.0 } else { left - 16.0 - size.x };
    let point = Pos2::new(x, anchor.y - 20.0);
    let min = Pos2::new(
        point.x.clamp(proj.rect.left(), (proj.rect.right() - size.x).max(proj.rect.left())),
        point.y.clamp(proj.rect.top(), (proj.rect.bottom() - size.y).max(proj.rect.top())),
    );
    Some(Rect::from_min_size(min, size))
}

fn paint_interface(ui: &egui::Ui, proj: &Proj, interface: &Interface, info: &aluminium_ports::MemberInfo, color: egui::Color32) {
    let (at, _, _) = info.frame.port(interface.port, interface.position, 0.0);
    if !interface.port.is_end() {
        let across = if matches!(interface.port, Port::XPositive | Port::XNegative) { info.frame.y } else { info.frame.x };
        let half = info.frame.size / 2.0;
        let opening =
            solidcraft_engine::doc::aluminium::MODELS.iter().find(|m| m.0 == info.frame.model).map_or(info.frame.size * 0.15, |m| m.2 / 2.0);
        let (lo, hi) = if interface.kind == "slot" {
            (-opening, opening)
        } else if interface.strip < 0.0 {
            (-half, -opening)
        } else {
            (opening, half)
        };
        let base = at - info.frame.axis * (info.frame.length * interface.position);
        let corners = [
            base + across * lo,
            base + across * hi,
            base + across * hi + info.frame.axis * info.frame.length,
            base + across * lo + info.frame.axis * info.frame.length,
        ];
        let points = corners.iter().filter_map(|point| proj.to_screen(*point)).collect::<Vec<_>>();
        if points.len() == 4 {
            let fill = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 28);
            ui.painter().add(egui::Shape::convex_polygon(points.clone(), fill, Stroke::new(2.0, color)));
        }
    } else if let Some(p) = proj.to_screen(at) {
        ui.painter().circle_filled(
            p,
            if interface.kind == "bore" { 7.0 } else { 12.0 },
            egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 35),
        );
        ui.painter().circle_stroke(p, if interface.kind == "bore" { 7.0 } else { 12.0 }, Stroke::new(2.0, color));
    }
}

pub(crate) fn owns_pointer(app: &SolveApp, proj: &Proj, point: Pos2) -> bool {
    if connectors::owns_pointer(app, proj, point) {
        return true;
    }
    if let Some((interface, info)) = selected(app) {
        return action_rect(&interface, &info, proj).is_some_and(|r| r.contains(point) && !crate::shell::covered(app, point));
    }
    if let Some(Kind::Aluminium(m)) = app.dialog.as_ref().map(|d| &d.kind) {
        return length_end(app, m, proj).is_some_and(|(base, _, _, end)| {
            end.distance(point) <= 13.0 || (m.attachment.is_some() && proj.to_screen(base).is_some_and(|p| p.distance(point) <= 13.0))
        });
    }
    false
}

pub(crate) fn show(app: &mut SolveApp, ui: &mut egui::Ui, proj: &Proj) {
    if let Some(cursor) = ui.input(|input| input.pointer.hover_pos())
        && let Some(interface) = interface_under_pointer(app, proj, cursor)
        && let Ok(info) = aluminium_ports::member(&app.session, &interface.body)
    {
        paint_interface(ui, proj, &interface, &info, Tokens::get().hover_profile_edge);
    }
    if let Some((interface, info)) = selected(app) {
        let tk = Tokens::get();
        let color = tk.accent;
        paint_interface(ui, proj, &interface, &info, tk.accent);
        if let Some(rect) = action_rect(&interface, &info, proj) {
            let label_rect = Rect::from_min_size(rect.min, vec2(rect.width(), 22.0));
            if let Some(anchor) = proj.to_screen(interface.point) {
                let edge = Pos2::new(if rect.center().x > anchor.x { rect.left() } else { rect.right() }, anchor.y.clamp(rect.top(), rect.bottom()));
                ui.painter().line_segment([anchor, edge], Stroke::new(1.0, color));
                ui.painter().circle_filled(anchor, 3.0, color);
            }
            ui.painter().rect_filled(rect, tk.control_radius, tk.overlay);
            ui.painter().text(
                label_rect.center(),
                Align2::CENTER_CENTER,
                crate::i18n::tr(&format!("interface.{}", interface.kind)),
                FontId::proportional(tk.font_caption),
                tk.text,
            );
            let edit = Rect::from_min_size(rect.min + vec2(3.0, 24.0), vec2(rect.width() - 6.0, 24.0));
            let extend = Rect::from_min_size(rect.min + vec2(3.0, 51.0), vec2(rect.width() - 6.0, 24.0));
            let parts = Rect::from_min_size(rect.min + vec2(3.0, 78.0), vec2(rect.width() - 6.0, 24.0));
            let edit_response = ui.put(edit, egui::Button::new(crate::i18n::tr("aluminium.edit_profile")));
            crate::scenario::publish_handle("aluminium:edit_profile", edit_response.rect.center());
            if edit_response.clicked()
                && let Some(body) = app.session.world_state().body(&interface.body)
            {
                app.edit_feature(body.feature);
            }
            if matches!(interface.kind, "surface" | "end") {
                let response = ui.put(extend, egui::Button::new(crate::i18n::tr("interface.add_member")));
                crate::scenario::publish_handle(&format!("aluminium:extend:{}", interface.port.key()), response.rect.center());
                if response.clicked() {
                    crate::dialogs_aluminium::attach(app, &interface.body, interface.port, interface.position);
                }
            }
            let response = ui.put(parts, egui::Button::new(crate::i18n::tr("interface.add_part")));
            crate::scenario::publish_handle("aluminium:interface:parts", response.rect.center());
            if response.clicked() {
                crate::dialogs_connectors::at_interface(app, &interface);
            }
        }
    }
    connectors::show(app, ui, proj);
    position_handle(app, ui, proj);
    length_handle(app, ui, proj);
    crate::dialogs_connectors::mount_guide(app, ui, proj);
}

fn length_end(
    app: &SolveApp,
    m: &crate::dialogs_aluminium::Profile,
    proj: &Proj,
) -> Option<(solidcraft_engine::geom::Vec3, solidcraft_engine::geom::Vec3, f64, Pos2)> {
    let command = crate::dialogs_aluminium::commands(m, &serde_json::Map::new()).ok()?.into_iter().next()?;
    let (base, axis, _) = aluminium_ports::placement(&app.session, &command.1).ok()?;
    let expression = app.session.doc.with_design_unit(&m.length, solidcraft_engine::doc::expr::Kind::Length);
    let length = app.session.doc.eval(&expression, solidcraft_engine::doc::expr::Kind::Length).ok()?;
    if !(0.1..=100_000.0).contains(&length) {
        return None;
    }
    let end = proj.to_screen(base + axis * length)?;
    Some((base, axis, length, end))
}

fn length_handle(app: &mut SolveApp, ui: &mut egui::Ui, proj: &Proj) {
    let Some(Kind::Aluminium(m)) = app.dialog.as_ref().map(|d| &d.kind) else {
        app.viewport.aluminium_drag = None;
        return;
    };
    let Some((base, axis, length, end)) = length_end(app, m, proj) else { return };
    let Some(start) = proj.to_screen(base) else { return };
    let tk = Tokens::get();
    ui.painter().line_segment([start, end], Stroke::new(2.0, tk.accent));
    ui.painter().circle(end, 7.0, tk.overlay, Stroke::new(2.0, tk.accent));
    ui.painter().text(end + vec2(12.0, -10.0), Align2::LEFT_BOTTOM, format!("{length:.1} mm"), FontId::proportional(tk.font_caption), tk.text);
    crate::scenario::publish_handle("aluminium:length", end);
    let response = ui.interact(Rect::from_center_size(end, Vec2::splat(24.0)), ui.id().with("aluminium_length"), Sense::drag());
    let along = |point| {
        let (o, d) = proj.ray(point);
        let c = axis.dot(d);
        let den = 1.0 - c * c;
        (den > 1e-6).then(|| ((o - base).dot(axis) - c * (o - base).dot(d)) / den)
    };
    if response.drag_started()
        && let Some(at) = ui.input(|i| i.pointer.press_origin()).and_then(along)
    {
        app.viewport.aluminium_drag = Some((at, length));
    }
    if (response.dragged() || response.drag_stopped())
        && let Some((from, initial)) = app.viewport.aluminium_drag
        && let Some(at) = response.interact_pointer_pos().and_then(along)
    {
        let length = (initial + at - from).clamp(0.1, 100_000.0);
        if let Some(Kind::Aluminium(m)) = app.dialog.as_mut().map(|d| &mut d.kind) {
            m.length = format!("{length:.2} mm");
        }
    }
    if response.drag_stopped() {
        app.viewport.aluminium_drag = None;
    }
    response.on_hover_text(crate::i18n::tr("aluminium.drag_length"));
}

fn position_handle(app: &mut SolveApp, ui: &mut egui::Ui, proj: &Proj) {
    let Some(Kind::Aluminium(m)) = app.dialog.as_ref().map(|d| &d.kind) else { return };
    let Ok(Some(mount)) = aluminium_ports::attachment(&app.session, m.attachment.as_ref()) else { return };
    if mount.port.is_end() {
        return;
    }
    let state = app.session.model.state();
    let Some((world, ..)) = app
        .session
        .doc
        .body_frames_with_source(&state)
        .into_iter()
        .find(|(_, source, _, path, _)| source == mount.body.as_ref() && *path == mount.path)
    else {
        return;
    };
    let Ok(info) = aluminium_ports::member(&app.session, &world) else { return };
    let position = app.session.doc.eval(&mount.position, solidcraft_engine::doc::expr::Kind::Unitless).unwrap_or(0.5);
    let gap = app
        .session
        .doc
        .eval(&app.session.doc.with_design_unit(&mount.gap, solidcraft_engine::doc::expr::Kind::Length), solidcraft_engine::doc::expr::Kind::Length)
        .unwrap_or(0.0);
    let (start, _, _) = info.frame.port(mount.port, 0.0, gap);
    let (at, _, _) = info.frame.port(mount.port, position, gap);
    let Some(point) = proj.to_screen(at) else { return };
    let color = Tokens::get().accent;
    ui.painter().circle_stroke(point, 7.0, Stroke::new(2.0, color));
    crate::scenario::publish_handle("aluminium:position", point);
    let response = ui.interact(Rect::from_center_size(point, Vec2::splat(24.0)), ui.id().with("aluminium_position"), Sense::drag());
    if (response.dragged() || response.drag_stopped())
        && let Some(p) = response.interact_pointer_pos()
    {
        let (origin, ray) = proj.ray(p);
        let axis = info.frame.axis;
        let dot = axis.dot(ray);
        let den = 1.0 - dot * dot;
        if den > 1e-6 {
            let distance = ((origin - start).dot(axis) - dot * (origin - start).dot(ray)) / den;
            let position = (distance / info.frame.length).clamp(0.0, 1.0);
            if let Some(Kind::Aluminium(m)) = app.dialog.as_mut().map(|d| &mut d.kind)
                && let Some(value) = &mut m.attachment
            {
                value["position"] = serde_json::json!(format!("{position:.6}"));
            }
        }
    }
    response.on_hover_text(crate::i18n::tr("aluminium.drag_position"));
}

/// The aluminium workspace works with members/parts and named interfaces, not B-rep topology.
pub(crate) fn part_selection_mode(app: &SolveApp) -> bool {
    app.ui.tab == "ALUMINIUM" && app.session.active_sketch.is_none() && app.tool.is_none()
}

pub(crate) fn whole_selection(app: &SolveApp, body: &str) -> Option<Sel> {
    if !part_selection_mode(app) {
        return None;
    }
    if body.is_empty() {
        return None;
    }
    let world = app.session.world_state();
    let Some(model) = world.body(body) else {
        return Some(Sel::Body { name: body.into() });
    };
    let member = matches!(
        app.session.doc.feature(model.feature).map(|f| &f.kind),
        Some(
            solidcraft_engine::doc::FeatureKind::AluminiumProfile { .. }
                | solidcraft_engine::doc::FeatureKind::FrameMember { profile: solidcraft_engine::doc::frame::MemberProfile::Stock { .. }, .. }
        )
    );
    Some(if member { Sel::Body { name: body.into() } } else { crate::viewport::component_selection(app, body) })
}

pub(crate) fn whole_hit(app: &SolveApp, hit: &crate::viewport::Hit) -> Option<Sel> {
    use crate::viewport::Hit;
    match hit {
        Hit::Face { body, .. } | Hit::Edge { body, .. } | Hit::Vertex { body, .. } => whole_selection(app, body),
        _ => None,
    }
}

pub(crate) fn normalize_pick(app: &SolveApp, item: Sel) -> Sel {
    let body = match &item {
        Sel::Body { name } => Some(name),
        Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => Some(body),
        _ => None,
    };
    body.and_then(|name| whole_selection(app, name)).unwrap_or(item)
}

/// Find the selected member's semantic interface under the pointer. The nominal envelope keeps
/// slots and bores interactive even where the B-rep has no face to hit.
fn interface_under_pointer(app: &SolveApp, proj: &Proj, point: Pos2) -> Option<Interface> {
    if !part_selection_mode(app) || app.dialog.is_some() {
        return None;
    }
    let [Sel::Body { name }] = app.session.selection.as_slice() else {
        return None;
    };
    if let Some(crate::viewport::Hit::Face { body, .. } | crate::viewport::Hit::Edge { body, .. } | crate::viewport::Hit::Vertex { body, .. }) =
        app.viewport.hover.as_ref()
        && body != name
    {
        return None;
    }
    let Ok(info) = aluminium_ports::member(&app.session, &name) else {
        return None;
    };
    let (origin, direction) = proj.ray(point);
    let delta = origin - info.frame.origin;
    let half = info.frame.size / 2.0;
    let mut enter: f64 = 0.0;
    let mut exit = f64::INFINITY;
    for (axis, lo, hi) in [(info.frame.x, -half, half), (info.frame.y, -half, half), (info.frame.axis, 0.0, info.frame.length)] {
        let o = delta.dot(axis);
        let d = direction.dot(axis);
        if d.abs() < 1e-9 {
            if o < lo || o > hi {
                return None;
            }
            continue;
        }
        let a = (lo - o) / d;
        let b = (hi - o) / d;
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
        if enter > exit {
            return None;
        }
    }
    if !enter.is_finite() {
        return None;
    }
    interface_from_point(app, name, origin + direction * enter)
}

/// Intersect the selected member's nominal envelope so empty slots and the bore are pickable.
pub(crate) fn pick_interface(app: &mut SolveApp, proj: &Proj, point: Pos2) -> bool {
    let Some(interface) = interface_under_pointer(app, proj, point) else { return false };
    remember_position(app, &interface.body, interface.point);
    true
}

pub(crate) fn remember_hit(app: &mut SolveApp, hit: &crate::viewport::Hit) {
    use crate::viewport::Hit;
    if !part_selection_mode(app) {
        return;
    }
    let (body, point) = match hit {
        Hit::Face { body, point, .. } | Hit::Edge { body, point, .. } | Hit::Vertex { body, point } => (body, *point),
        _ => {
            app.viewport.aluminium_interface = None;
            return;
        }
    };
    remember_position(app, body, point);
    // A first click selects the part. Installation actions require a second, deliberate pick.
    app.viewport.aluminium_interface = None;
}

fn remember_position(app: &mut SolveApp, name: &str, point: solidcraft_engine::geom::Vec3) {
    if let Some(interface) = interface_from_point(app, name, point) {
        app.viewport.aluminium_station = Some((name.into(), interface.position));
        app.viewport.aluminium_interface = Some(interface);
    }
}

fn interface_from_point(app: &SolveApp, name: &str, point: solidcraft_engine::geom::Vec3) -> Option<Interface> {
    if let Ok(info) = aluminium_ports::member(&app.session, name) {
        let position = ((point - info.frame.origin).dot(info.frame.axis) / info.frame.length).clamp(0.0, 1.0);
        let delta = point - info.frame.origin;
        let x = delta.dot(info.frame.x);
        let y = delta.dot(info.frame.y);
        let z = delta.dot(info.frame.axis);
        let end = z.min(info.frame.length - z) < 0.25;
        let dimensions = solidcraft_engine::doc::aluminium::MODELS.iter().find(|m| m.0 == info.frame.model);
        let bore = dimensions.map_or(2.5, |m| m.3 / 2.0);
        let opening = dimensions.map_or(3.1, |m| m.2 / 2.0);
        let (port, kind, strip) = if end || (x * x + y * y).sqrt() < bore + 0.1 {
            (if z < info.frame.length / 2.0 { Port::Start } else { Port::End }, if (x * x + y * y).sqrt() < bore + 0.1 { "bore" } else { "end" }, 0.0)
        } else {
            let (port, across) = if x.abs() > y.abs() {
                (if x > 0.0 { Port::XPositive } else { Port::XNegative }, y)
            } else {
                (if y > 0.0 { Port::YPositive } else { Port::YNegative }, x)
            };
            (port, if across.abs() < opening { "slot" } else { "surface" }, across)
        };
        return Some(Interface { body: name.into(), port, kind, position, point, strip });
    }
    None
}

pub(crate) fn sync_selection(app: &mut SolveApp) {
    let active = part_selection_mode(app);
    let entered = active && !app.viewport.aluminium_part_selection;
    app.viewport.aluminium_part_selection = active;
    if app
        .viewport
        .aluminium_interface
        .as_ref()
        .is_some_and(|interface| !matches!(app.session.selection.as_slice(),[Sel::Body{name}] if name==&interface.body))
    {
        app.viewport.aluminium_interface = None;
    }
    if !active || app.applying.is_some() {
        return;
    }
    let raw = app.session.selection.iter().any(|s| matches!(s, Sel::Face { .. } | Sel::Edge { .. } | Sel::Vertex { .. }));
    if !entered && !raw {
        return;
    }
    let before = app.session.selection.clone();
    let mut items = Vec::new();
    for item in &before {
        if let Sel::Face { body, point, .. } | Sel::Edge { body, point, .. } | Sel::Vertex { body, point } = item {
            remember_position(app, body, *point);
        }
        let item = normalize_pick(app, item.clone());
        if !items.contains(&item) {
            items.push(item);
        }
    }
    if items != before {
        let _ = app.run("select.set", serde_json::json!({"items":items}));
    }
}

pub(crate) fn hover_value(app: &SolveApp) -> Option<serde_json::Value> {
    let hit = app.viewport.hover.as_ref()?;
    match whole_hit(app, hit) {
        Some(item) => serde_json::to_value(item).ok(),
        None => serde_json::to_value(hit).ok(),
    }
}

pub(crate) fn remember_selection(app: &mut SolveApp, item: &Sel) {
    if !part_selection_mode(app) {
        return;
    }
    if let Sel::Face { body, point, .. } | Sel::Edge { body, point, .. } | Sel::Vertex { body, point } = item {
        remember_position(app, body, *point);
    }
}
