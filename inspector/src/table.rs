use std::collections::{BTreeMap, BTreeSet};

use comfy_table::{ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use serde::Serialize;
use serde_json::{Map, Value};

pub(crate) fn serialize<T: Serialize>(value: T) -> anyhow::Result<String> {
    Ok(render(&serde_json::to_value(value)?))
}

fn render(value: &Value) -> String {
    match value {
        Value::Array(values) => render_list(values),
        Value::Object(values) => render_object(values),
        _ => scalar_to_string(value),
    }
}

fn render_object(values: &Map<String, Value>) -> String {
    let mut fields = BTreeMap::new();
    let mut lists = Vec::new();
    flatten_object(None, values, &mut fields, &mut lists);

    let mut sections = vec![render_detail_box(&fields)];
    sections.extend(
        lists
            .into_iter()
            .map(|(path, values)| format!("{path}\n{}", render_list(values))),
    );
    sections.join("\n\n")
}

fn render_list(values: &[Value]) -> String {
    if values.is_empty() {
        return "(empty)".to_string();
    }

    if values.iter().all(Value::is_object) {
        render_object_list(values)
    } else {
        render_value_list(values)
    }
}

fn render_object_list(values: &[Value]) -> String {
    let rows = values
        .iter()
        .map(|value| {
            let mut fields = BTreeMap::new();
            flatten_list_row(
                value.as_object().expect("object list only contains objects"),
                &mut fields,
            );
            fields
        })
        .collect::<Vec<_>>();
    let columns = rows.iter().flat_map(BTreeMap::keys).collect::<BTreeSet<_>>();
    let mut table = new_table();
    let mut header = vec!["INDEX"];
    header.extend(columns.iter().map(|field| field.as_str()));
    table.set_header(header);

    for (index, fields) in rows.iter().enumerate() {
        let mut row = vec![index.to_string()];
        row.extend(
            columns
                .iter()
                .map(|field| fields.get(*field).cloned().unwrap_or_else(|| "-".to_string())),
        );
        table.add_row(row);
    }

    table.to_string()
}

fn render_value_list(values: &[Value]) -> String {
    let mut table = new_table();
    table
        .set_header(["VALUE"])
        .add_rows(values.iter().map(|value| [inline_value(value)]))
        .to_string()
}

fn render_detail_box(fields: &BTreeMap<String, String>) -> String {
    let mut table = new_table();
    table
        .set_header(["FIELD", "VALUE"])
        .add_rows(fields.iter().map(|(field, value)| [field, value]))
        .to_string()
}

fn flatten_object<'a>(
    prefix: Option<&str>,
    values: &'a Map<String, Value>,
    fields: &mut BTreeMap<String, String>,
    lists: &mut Vec<(String, &'a [Value])>,
) {
    for (field, value) in values {
        let path = dotted_path(prefix, field);
        match value {
            Value::Object(values) => flatten_object(Some(&path), values, fields, lists),
            Value::Array(values) if values.iter().all(Value::is_object) && !values.is_empty() => {
                fields.insert(path.clone(), format!("({} rows below)", values.len()));
                lists.push((path, values));
            }
            _ => {
                fields.insert(path, inline_value(value));
            }
        }
    }
}

fn flatten_list_row(values: &Map<String, Value>, fields: &mut BTreeMap<String, String>) {
    for (field, value) in values {
        flatten_list_row_value(field, value, fields);
    }
}

fn flatten_list_row_value(prefix: &str, value: &Value, fields: &mut BTreeMap<String, String>) {
    if omit_list_column(prefix) {
        return;
    }

    match value {
        Value::Object(values) => {
            for (field, value) in values {
                flatten_list_row_value(&dotted_path(Some(prefix), field), value, fields);
            }
        }
        _ => {
            fields.insert(prefix.to_string(), inline_value(value));
        }
    }
}

fn omit_list_column(field: &str) -> bool {
    matches!(
        field,
        "destination.packet_key" | "destination.peer_id" | "destination.safe_address"
    )
}

fn inline_value(value: &Value) -> String {
    match value {
        Value::Null => "-".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) if values.is_empty() => "(empty)".to_string(),
        Value::Array(values) => values.iter().map(inline_value).collect::<Vec<_>>().join(", "),
        Value::Object(values) => serde_json::to_string(values).expect("JSON object serialization cannot fail"),
    }
}

fn scalar_to_string(value: &Value) -> String {
    inline_value(value)
}

fn dotted_path(parent: Option<&str>, child: &str) -> String {
    parent.map_or_else(|| child.to_string(), |parent| format!("{parent}.{child}"))
}

fn new_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::render;

    #[test]
    fn renders_scalar() {
        insta::assert_snapshot!(render(&json!("healthy")));
    }

    #[test]
    fn renders_detail_box() {
        insta::assert_snapshot!(render(&json!({
            "address": "0x1234",
            "enabled": true,
            "optional": null,
        })));
    }

    #[test]
    fn renders_flat_list() {
        insta::assert_snapshot!(render(&json!([
            {"keyid": 1, "packet_key": "first"},
            {"keyid": 2, "packet_key": "second", "safe_address": "0x1234"},
        ])));
    }

    #[test]
    fn flattens_nested_objects_and_inline_lists() {
        insta::assert_snapshot!(render(&json!({
            "source": {
                "chain_key": "0x1234",
                "multi_addresses": ["/ip4/127.0.0.1/tcp/9091"],
            },
            "channels": [
                {
                    "channel": {"status": "OPEN", "ticket_index": "0"},
                    "destination": {
                        "chain_key": "0x1234",
                        "packet_key": "packet-key",
                        "peer_id": "12D3KooWFullPeerId",
                        "safe_address": "0xabcd",
                        "multi_addresses": ["/ip4/127.0.0.1/tcp/9091"],
                    },
                },
            ],
            "empty": [],
        })));
    }

    #[test]
    fn preserves_long_values() {
        insta::assert_snapshot!(render(&json!({
            "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        })));
    }
}
