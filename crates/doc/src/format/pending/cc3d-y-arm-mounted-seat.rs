//! A placed CC3D20 y-positive arm is 6 mm short of its profile shoulder in older
//! designs. Move that connector's saved root occurrence along its local +Y axis
//! when they open; the previous release does not know this format and asks for
//! an update instead of silently keeping the old placement.
use serde_json::Value;

pub fn step(v: &mut Value) -> crate::Result<()> {
    let mut mounted_y_components = Vec::new();
    if let Some(components) = v.get("components").and_then(Value::as_array) {
        for component in components {
            let id = component.get("id").and_then(Value::as_u64);
            let properties = component.get("properties");
            let model = properties.and_then(|p| p.get("stock_connector")).and_then(Value::as_str);
            let mount = properties
                .and_then(|p| p.get("stock_mount"))
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            if model == Some("motedis-cc3d20x20m4x5-6c2938c7")
                && mount.as_ref().and_then(|m| m.get("arm")).and_then(Value::as_str) == Some("y_positive")
                && let Some(id) = id
            {
                mounted_y_components.push(id);
            }
        }
    }
    if mounted_y_components.is_empty() {
        return Ok(());
    }

    if let Some(occurrences) = v.get_mut("occurrences").and_then(Value::as_array_mut) {
        for occurrence in occurrences {
            if !occurrence
                .get("component")
                .and_then(Value::as_u64)
                .is_some_and(|id| mounted_y_components.contains(&id))
            {
                continue;
            }
            let Some(rows) = occurrence.get_mut("transform").and_then(Value::as_array_mut) else { continue };
            if rows.len() != 4 || rows.iter().any(|row| row.as_array().is_none_or(|values| values.len() != 4)) {
                continue;
            }

            // Serialized transforms are row-major; local basis vectors are columns.
            for world_axis in 0..3 {
                let local_y_axis = rows[world_axis][1].as_f64().unwrap_or(0.0);
                let translation = rows[3][world_axis].as_f64().unwrap_or(0.0);
                rows[3][world_axis] = Value::from(translation + 6.0 * local_y_axis);
            }
        }
    }
    Ok(())
}
