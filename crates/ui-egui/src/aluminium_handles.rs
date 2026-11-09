//! Direct model interfaces: extend a stock member from a face or place a connector at an end.
use crate::{SolveApp, dialogs::Kind, theme::Tokens, viewport::Proj};
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2, vec2};
use solidcraft_engine::{Sel, aluminium_ports, doc::aluminium::Port};

struct Handle {
    point: Pos2,
    anchor: Pos2,
    port: Port,
    connector: bool,
}

fn selected(app: &SolveApp) -> Option<(String, aluminium_ports::MemberInfo, f64)> {
    if app.dialog.is_some() || app.tool.is_some() || app.session.active_sketch.is_some() || app.applying.is_some() || app.ui.tab != "ALUMINIUM" {
        return None;
    }
    if app.session.selection.len() != 1 {
        return None;
    }
    let selection = app.session.selection.first()?;
    let (name, picked) = match selection {
        Sel::Body { name } => (name.clone(), None),
        Sel::Face { body, point, .. } | Sel::Edge { body, point, .. } | Sel::Vertex { body, point, .. } => (body.clone(), Some(*point)),
        _ => return None,
    };
    if !app.body_visible(&name) {
        return None;
    }
    let info = aluminium_ports::member(&app.session, &name).ok()?;
    let remembered = app.viewport.aluminium_station.as_ref().filter(|(body, _)| body == &name).map_or(0.5, |(_, position)| *position);
    let position = picked.map_or(remembered, |p| ((p - info.frame.origin).dot(info.frame.axis) / info.frame.length).clamp(0.0, 1.0));
    Some((name, info, position))
}

fn handles(info: &aluminium_ports::MemberInfo, position: f64, proj: &Proj) -> Vec<Handle> {
    let mut handles = Vec::new();
    for port in Port::ALL {
        let (at, normal, _) = info.frame.port(port, position, 0.0);
        let Some(anchor) = proj.to_screen(at) else { continue };
        let direction = proj.to_screen(at + normal * (info.frame.size.max(20.0))).map(|p| (p - anchor).normalized()).unwrap_or(vec2(0.0, -1.0));
        let direction = if direction.length_sq() > 0.01 { direction } else { vec2(0.0, -1.0) };
        let point = anchor + direction * if port.is_end() { 22.0 } else { 30.0 };
        if proj.rect.shrink(14.0).contains(point) {
            handles.push(Handle { point, anchor, port, connector: false });
        }
        if port.is_end() {
            let point = point + vec2(-direction.y, direction.x) * 28.0;
            if proj.rect.shrink(14.0).contains(point) {
                handles.push(Handle { point, anchor, port, connector: true });
            }
        }
    }
    handles
}

pub(crate) fn owns_pointer(app: &SolveApp, proj: &Proj, point: Pos2) -> bool {
    if let Some((_, info, position)) = selected(app) {
        return handles(&info, position, proj).iter().any(|h| h.point.distance(point) <= 13.0 && !crate::shell::covered(app, point));
    }
    if let Some(Kind::Aluminium(m)) = app.dialog.as_ref().map(|d| &d.kind) {
        return length_end(app, m, proj).is_some_and(|(base, _, _, end)| {
            end.distance(point) <= 13.0 || (m.attachment.is_some() && proj.to_screen(base).is_some_and(|p| p.distance(point) <= 13.0))
        });
    }
    false
}

pub(crate) fn show(app: &mut SolveApp, ui: &mut egui::Ui, proj: &Proj) {
    if let Some((name, info, position)) = selected(app) {
        let tk = Tokens::get();
        for h in handles(&info, position, proj) {
            if crate::shell::covered(app, h.point) {
                continue;
            }
            let rect = Rect::from_center_size(h.point, Vec2::splat(24.0));
            let action = if h.connector { "connector" } else { "extend" };
            let response = ui.interact(rect, ui.id().with(("aluminium_port", &name, h.port.key(), action)), Sense::click());
            crate::scenario::publish_handle(&format!("aluminium:{action}:{}", h.port.key()), h.point);
            let color = if h.connector { tk.joint_manipulator } else { tk.accent };
            let painter = ui.painter();
            painter.line_segment([h.anchor, h.point], Stroke::new(1.2, color));
            painter.circle_filled(h.anchor, 3.0, color);
            painter.circle(h.point, 10.0, if response.hovered() { color } else { tk.overlay }, Stroke::new(1.5, color));
            if h.connector {
                crate::icons::paint_icon(painter, rect.shrink(5.0), "joint", 1.0);
            } else {
                painter.text(
                    h.point,
                    Align2::CENTER_CENTER,
                    "+",
                    FontId::proportional(tk.font_heading),
                    if response.hovered() { tk.on_accent } else { color },
                );
            }
            let tip = if h.connector { "aluminium.handle_connector" } else { "aluminium.handle_extend" };
            let clicked = response.on_hover_text(crate::i18n::tr(tip)).clicked();
            if clicked {
                if h.connector {
                    crate::dialogs_connectors::at_port(app, &name, h.port);
                } else {
                    crate::dialogs_aluminium::attach(app, &name, h.port, position);
                }
                break;
            }
        }
    }
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

pub(crate) fn remember_hit(app: &mut SolveApp, hit: &crate::viewport::Hit) {
    use crate::viewport::Hit;
    if !part_selection_mode(app) {
        return;
    }
    let (body, point) = match hit {
        Hit::Face { body, point, .. } | Hit::Edge { body, point, .. } | Hit::Vertex { body, point } => (body, *point),
        _ => return,
    };
    remember_position(app, body, point);
}

fn remember_position(app: &mut SolveApp, name: &str, point: solidcraft_engine::geom::Vec3) {
    if let Ok(info) = aluminium_ports::member(&app.session, name) {
        let position = ((point - info.frame.origin).dot(info.frame.axis) / info.frame.length).clamp(0.0, 1.0);
        app.viewport.aluminium_station = Some((name.into(), position));
    }
}

pub(crate) fn sync_selection(app: &mut SolveApp) {
    let active = part_selection_mode(app);
    let entered = active && !app.viewport.aluminium_part_selection;
    app.viewport.aluminium_part_selection = active;
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
