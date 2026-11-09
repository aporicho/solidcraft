//! Stock connectors are fixed manufacturer models; the tool only places them.
use serde_json::{Map, Value, json};
use solidcraft_engine::stock_connectors::{self, Connector};

use crate::ui_kit::{choice_row, expr_row};
use crate::{SolveApp, dialogs::Kind, i18n, selection::SelInput};

#[derive(Clone, Debug)]
pub struct StockConnector {
    models: Vec<Connector>,
    selected: usize,
    origin: [String; 3],
    rotation: [String; 3],
}

pub fn start(_: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    if id != "aluminium.connector" {
        return None;
    }
    Some((
        Kind::AluminiumConnector(StockConnector {
            models: stock_connectors::catalogue().ok()?,
            selected: 0,
            origin: ["0".into(), "0".into(), "0".into()],
            rotation: ["0 deg".into(), "0 deg".into(), "0 deg".into()],
        }),
        Vec::new(),
    ))
}

pub fn rows(ui: &mut egui::Ui, state: &mut StockConnector) -> bool {
    let mut enter = false;
    let names: Vec<_> = state.models.iter().map(|model| format!("{} · {} · {}", model.series, model.part_number, model.supplier)).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    choice_row(ui, i18n::tr("connector.model"), "aluminium_connector_model", &mut state.selected, &names);
    if let Some(model) = state.models.get(state.selected) {
        ui.label(i18n::format("connector.slot", &[("slot", &model.slot.to_string())]));
        ui.hyperlink_to(i18n::tr("connector.source"), &model.product_url);
    }
    ui.label(i18n::tr("connector.position"));
    for (label, value) in ["X", "Y", "Z"].into_iter().zip(&mut state.origin) {
        enter |= expr_row(ui, label, value);
    }
    ui.label(i18n::tr("connector.rotation"));
    for (label, value) in ["X", "Y", "Z"].into_iter().zip(&mut state.rotation) {
        enter |= expr_row(ui, label, value);
    }
    ui.label(egui::RichText::new(i18n::tr("connector.fixed_note")).small().color(crate::theme::Tokens::get().form_dim));
    enter
}

pub fn commands(app: &SolveApp, state: &StockConnector, extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let model = state.models.get(state.selected).ok_or("choose a connector model")?;
    let mut params = json!({"model":model.id,"origin":state.origin,"rotation":state.rotation});
    if let Some(name) = extra.get("name") {
        params["name"] = name.clone();
    }
    let params = stock_connectors::import_params(&app.session, &params).map_err(|error| error.to_string())?;
    Ok(vec![("file.insert_step".into(), params)])
}
