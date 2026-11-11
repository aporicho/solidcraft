//! The CC3D20 y-positive tongue's actual shoulder is 6 mm deeper than the
//! previous saved datum. Move mounted connectors and their saved y-arm datums
//! to that shoulder when older designs open; older releases refuse this marker.
use serde_json::Value;

const MODEL: &str = "motedis-cc3d20x20m4x5-6c2938c7";

fn vec3(value: Option<&Value>) -> Option<[f64; 3]> {
    let values = value?.as_array()?;
    if values.len() != 3 {
        return None;
    }
    let out = [values[0].as_f64()?, values[1].as_f64()?, values[2].as_f64()?];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

fn length_mm(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    let parsed = value.as_f64().or_else(|| {
        let text = value.as_str()?.trim();
        let number = text.strip_suffix("mm").unwrap_or(text).trim();
        number.parse().ok()
    })?;
    parsed.is_finite().then_some(parsed)
}

fn apply_point(columns: &[Value], point: [f64; 3]) -> Option<[f64; 3]> {
    let axis = |index: usize| -> Option<f64> {
        Some(
            columns[0].get(index)?.as_f64()? * point[0]
                + columns[1].get(index)?.as_f64()? * point[1]
                + columns[2].get(index)?.as_f64()? * point[2]
                + columns[3].get(index)?.as_f64()?,
        )
    };
    Some([axis(0)?, axis(1)?, axis(2)?])
}

fn apply_vector(columns: &[Value], vector: [f64; 3]) -> Option<[f64; 3]> {
    let axis = |index: usize| -> Option<f64> {
        Some(columns[0].get(index)?.as_f64()? * vector[0] + columns[1].get(index)?.as_f64()? * vector[1] + columns[2].get(index)?.as_f64()? * vector[2])
    };
    Some([axis(0)?, axis(1)?, axis(2)?])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(v: [f64; 3], amount: f64) -> [f64; 3] {
    [v[0] * amount, v[1] * amount, v[2] * amount]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot(v, v).sqrt();
    (length.is_finite() && length > 1e-12).then(|| scale(v, length.recip()))
}

fn profile_endpoint(features: &[Value], body: &str, end: &str, position: f64) -> Option<([f64; 3], [f64; 3], u64)> {
    let feature = features.iter().find(|feature| {
        feature.get("type").and_then(Value::as_str) == Some("aluminium_profile")
            && feature
                .get("body_names")
                .and_then(Value::as_array)
                .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(body)))
    })?;
    let origin = vec3(feature.get("origin"))?;
    let axis = unit(vec3(feature.get("axis"))?)?;
    let length = length_mm(feature.get("length"))?;
    let distance = length * position;
    let endpoint = [origin[0] + axis[0] * distance, origin[1] + axis[1] * distance, origin[2] + axis[2] * distance];
    let inward = if end == "start" { axis } else { scale(axis, -1.0) };
    Some((endpoint, inward, feature.get("component").and_then(Value::as_u64).unwrap_or(0)))
}

fn arm_from_parent_end(transform: &[Value], endpoint: [f64; 3], inward: [f64; 3]) -> Option<(&'static str, [f64; 3])> {
    let candidates = [
        ("x_positive", [10.0, 10.0, 0.0], [1.0, 0.0, 0.0]),
        ("y_positive", [0.0, 10.0, 0.0], [0.0, 1.0, 0.0]),
        ("z_positive", [0.0, 10.0, 10.0], [0.0, 0.0, 1.0]),
    ];
    candidates
        .into_iter()
        .filter_map(|(name, local_origin, local_normal)| {
            let origin = apply_point(transform, local_origin)?;
            let normal = unit(apply_vector(transform, local_normal)?)?;
            let alignment = dot(normal, inward);
            let distance = dot(sub(origin, endpoint), sub(origin, endpoint)).sqrt();
            (alignment > 0.98 && distance.is_finite() && distance < 32.0).then_some((name, normal, distance))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(name, normal, _)| (name, normal))
}

pub fn step(v: &mut Value) -> crate::Result<()> {
    let Some(features) = v.get("features").and_then(Value::as_array).cloned() else { return Ok(()) };
    let mut mounted: Vec<(u64, String, String, f64)> = Vec::new();
    if let Some(components) = v.get("components").and_then(Value::as_array) {
        for component in components {
            let Some(id) = component.get("id").and_then(Value::as_u64) else { continue };
            let properties = component.get("properties");
            if properties.and_then(|p| p.get("stock_connector")).and_then(Value::as_str) != Some(MODEL) {
                continue;
            }
            let mount = properties
                .and_then(|p| p.get("stock_mount"))
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str::<Value>(text).ok());
            let Some(mount) = mount else { continue };
            let Some(body) = mount.get("body").and_then(Value::as_str) else { continue };
            let end = mount.get("port").and_then(Value::as_str).unwrap_or("end");
            if !matches!(end, "start" | "end") || mount.get("interface").and_then(Value::as_str).unwrap_or("end") != "end" {
                continue;
            }
            let position = mount.get("position").and_then(Value::as_f64).filter(|n| n.is_finite() && (0.0..=1.0).contains(n)).unwrap_or(1.0);
            mounted.push((id, body.to_owned(), end.to_owned(), position));
        }
    }

    let mut updated_arms = Vec::new();
    for (component_id, body, end, position) in mounted {
        let Some((endpoint, inward, parent_component)) = profile_endpoint(&features, &body, &end, position) else { continue };
        let Some(occurrences) = v.get_mut("occurrences").and_then(Value::as_array_mut) else { break };
        for occurrence in occurrences.iter_mut().filter(|occurrence| {
            occurrence.get("component").and_then(Value::as_u64) == Some(component_id)
                && occurrence.get("parent").and_then(Value::as_u64) == Some(parent_component)
        }) {
            let Some(columns) = occurrence.get_mut("transform").and_then(Value::as_array_mut) else { continue };
            if columns.len() != 4 || columns.iter().any(|column| column.as_array().is_none_or(|values| values.len() != 4)) {
                continue;
            }
            let Some((actual_arm, normal)) = arm_from_parent_end(columns, endpoint, inward) else { continue };
            let Some(port_origin) = apply_point(columns, [0.0, 10.0, 0.0]) else { continue };
            // The actual shoulder lies 6 mm behind the old y=10 datum. Put that shoulder on
            // the profile end, using the transformed port rather than saved arm metadata: a
            // later manual rotation may have left stock_mount.arm stale.
            let shoulder = sub(port_origin, scale(normal, 6.0));
            let correction = sub(endpoint, shoulder);
            for axis in 0..3 {
                let translation = columns[3][axis].as_f64().unwrap_or(0.0);
                columns[3][axis] = Value::from(translation + correction[axis]);
            }
            updated_arms.push((component_id, actual_arm));
        }
    }

    if let Some(components) = v.get_mut("components").and_then(Value::as_array_mut) {
        for (component_id, actual_arm) in updated_arms {
            let Some(component) = components.iter_mut().find(|component| component.get("id").and_then(Value::as_u64) == Some(component_id)) else {
                continue;
            };
            if let Some(properties) = component.get_mut("properties").and_then(Value::as_object_mut)
                && let Some(Value::String(serialized)) = properties.get_mut("stock_mount")
                && let Ok(mut mount) = serde_json::from_str::<Value>(serialized)
            {
                mount["arm"] = Value::from(actual_arm);
                if let Ok(updated) = serde_json::to_string(&mount) {
                    *serialized = updated;
                }
            }
        }
    }

    if let Some(features) = v.get_mut("features").and_then(Value::as_array_mut) {
        for feature in features {
            let Some(datum) = feature.get_mut("attachment").and_then(|attachment| attachment.get_mut("datum")) else { continue };
            if datum.get("model").and_then(Value::as_str) == Some(MODEL)
                && datum.get("port").and_then(Value::as_str) == Some("y_positive")
                && vec3(datum.get("origin")) == Some([0.0, 10.0, 0.0])
                && let Some(origin) = datum.get_mut("origin").and_then(Value::as_array_mut)
            {
                origin[1] = Value::from(4.0);
            }
        }
    }
    Ok(())
}
