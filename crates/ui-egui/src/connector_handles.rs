//! Free and connected installation ports on whole library components.
use crate::{SolveApp, theme::Tokens, viewport::Proj};
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use solidcraft_engine::{
    Sel,
    connector_ports::{self, WorldPort},
};

fn selected(app: &SolveApp) -> Option<(String, Vec<WorldPort>)> {
    if app.ui.tab != "ALUMINIUM" || app.dialog.is_some() || app.tool.is_some() || app.applying.is_some() || app.session.active_sketch.is_some() {
        return None;
    }
    let [item] = app.session.selection.as_slice() else { return None };
    let frames = app.session.doc.body_frames_with_source(&app.session.model.state());
    let body = match item {
        Sel::Body { name } => name.clone(),
        Sel::Occurrence { path } => frames.iter().filter(|(_, _, _, p, _)| p.starts_with(path)).find_map(|(world, ..)| {
            connector_ports::instance(&app.session, world).ok().filter(|i| !connector_ports::definitions(&i.model).is_empty()).map(|_| world.clone())
        })?,
        _ => return None,
    };
    let instance = connector_ports::instance(&app.session, &body).ok()?;
    Some((body, instance.ports(&app.session)))
}

fn point(port: &WorldPort, proj: &Proj) -> Option<Pos2> {
    let v = solidcraft_engine::geom::Vec3::new(port.origin[0], port.origin[1], port.origin[2]);
    proj.to_screen(v).filter(|p| proj.rect.shrink(14.0).contains(*p))
}

fn edit_rect(app: &SolveApp, body: &str, ports: &[WorldPort], proj: &Proj) -> Option<Rect> {
    let instance = connector_ports::instance(&app.session, body).ok()?;
    instance.mount.as_ref()?;
    let anchor = ports.iter().find_map(|port| point(port, proj))?;
    let size = egui::vec2(142.0, 25.0);
    let x = if anchor.x + 24.0 + size.x < proj.rect.right() { anchor.x + 24.0 } else { anchor.x - 24.0 - size.x };
    let min = Pos2::new(
        x.clamp(proj.rect.left(), (proj.rect.right() - size.x).max(proj.rect.left())),
        (anchor.y + 22.0).clamp(proj.rect.top(), proj.rect.bottom() - size.y),
    );
    Some(Rect::from_min_size(min, size))
}

pub(super) fn owns_pointer(app: &SolveApp, proj: &Proj, p: Pos2) -> bool {
    selected(app).is_some_and(|(body, ports)| {
        ports.iter().any(|port| point(port, proj).is_some_and(|q| p.distance(q) <= 14.0 && !crate::shell::covered(app, p)))
            || edit_rect(app, &body, &ports, proj).is_some_and(|rect| rect.contains(p) && !crate::shell::covered(app, p))
    })
}

pub(super) fn show(app: &mut SolveApp, ui: &mut egui::Ui, proj: &Proj) {
    let Some((body, ports)) = selected(app) else { return };
    let tk = Tokens::get();
    for (index, port) in ports.iter().enumerate() {
        let id = port.id.as_str();
        let Some(p) = point(port, proj).filter(|p| !crate::shell::covered(app, *p)) else { continue };
        let supported =
            solidcraft_engine::doc::aluminium::MODELS.iter().any(|m| m.1 == port.size && port.slot == if port.size == 20.0 { 6 } else { 8 });
        let free = port.occupied.is_none() && supported;
        let color = if free { tk.accent } else { tk.text_dim };
        let response = ui.interact(Rect::from_center_size(p, Vec2::splat(26.0)), ui.id().with(("connector_port", &body, id)), Sense::click());
        ui.painter().circle(p, 10.0, if response.hovered() { tk.hover } else { tk.overlay }, Stroke::new(2.0, color));
        ui.painter().text(p, Align2::CENTER_CENTER, (index + 1).to_string(), FontId::proportional(tk.font_caption), color);
        crate::scenario::publish_handle(&format!("connector_port:{id}"), p);
        crate::scenario::publish_handle(&format!("connector_port:{}:{id}", if port.occupied.is_some() { "connected" } else { "free" }), p);
        let hint = match &port.occupied {
            Some(member) => crate::i18n::format("ports.connected", &[("member", member)]),
            None => crate::i18n::tr(if supported { "ports.add_member" } else { "ports.unsupported_stock" }).to_owned(),
        };
        if response.on_hover_text(hint).clicked() && free {
            crate::dialogs_aluminium::attach_connector(app, &body, id);
            break;
        }
    }
    if let Some(rect) = edit_rect(app, &body, &ports, proj) {
        let response = ui.put(rect, egui::Button::new(crate::i18n::tr("aluminium.edit_connector_mount")));
        crate::scenario::publish_handle("connector:edit_mount", response.rect.center());
        if response.clicked() {
            crate::dialogs_connectors::edit_selected(app, &body);
        }
    }
}
