//! Move the saved CC3D20 y-positive attachment datum from its set-screw station
//! to the profile shoulder. Older designs keep the 6 mm shallow insertion.
use serde_json::Value;

pub fn step(v: &mut Value) -> crate::Result<()> {
    let Some(features) = v.get_mut("features").and_then(Value::as_array_mut) else { return Ok(()) };
    for feature in features {
        let Some(datum) = feature.get_mut("attachment").and_then(|a| a.get_mut("datum")) else { continue };
        let old = datum.get("origin").and_then(Value::as_array).is_some_and(|origin| {
            origin.len() == 3
                && origin[0].as_f64() == Some(0.0)
                && origin[1].as_f64() == Some(16.0)
                && origin[2].as_f64() == Some(0.0)
        });
        if datum.get("model").and_then(Value::as_str) == Some("motedis-cc3d20x20m4x5-6c2938c7")
            && datum.get("port").and_then(Value::as_str) == Some("y_positive")
            && old
            && let Some(origin) = datum.get_mut("origin").and_then(Value::as_array_mut)
        {
            origin[1] = Value::from(10.0);
        }
    }
    Ok(())
}
