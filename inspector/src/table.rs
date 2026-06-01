use std::collections::BTreeSet;

use comfy_table::Table;
use serde::Serialize;
use serde_json::{Map, Value};

pub(crate) fn serialize<T: Serialize>(value: T) -> anyhow::Result<String> {
    Ok(render(&serde_json::to_value(value)?))
}

fn render(value: &Value) -> String {
    let mut sections = Vec::new();
    render_section(None, value, &mut sections);
    sections.join("\n\n")
}

fn render_section(label: Option<&str>, value: &Value, sections: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            let (rendered, nested_values) = render_list(label, values);
            sections.push(with_label(label, rendered));
            render_nested_values(nested_values, sections);
        }
        Value::Object(values) => {
            let (rendered, nested_values) = render_object(label, values);
            sections.push(with_label(label, rendered));
            render_nested_values(nested_values, sections);
        }
        _ => sections.push(with_label(label, scalar_to_string(value))),
    }
}

fn render_nested_values(nested_values: Vec<(String, &Value)>, sections: &mut Vec<String>) {
    for (path, value) in nested_values {
        render_section(Some(&path), value, sections);
    }
}

fn render_object<'a>(label: Option<&str>, values: &'a Map<String, Value>) -> (String, Vec<(String, &'a Value)>) {
    let mut table = new_table();
    table.set_header(["FIELD", "VALUE"]);
    let mut nested_values = Vec::new();

    for (field, value) in values {
        if is_nested(value) {
            table.add_row([field, child_marker(value)]);
            nested_values.push((child_path(label, field), value));
        } else {
            table.add_row([field.to_string(), scalar_to_string(value)]);
        }
    }

    (table.to_string(), nested_values)
}

fn render_list<'a>(label: Option<&str>, values: &'a [Value]) -> (String, Vec<(String, &'a Value)>) {
    if values.is_empty() {
        return ("(empty)".to_string(), Vec::new());
    }

    if values.iter().all(Value::is_object) {
        render_object_list(label, values)
    } else {
        render_value_list(label, values)
    }
}

fn render_object_list<'a>(label: Option<&str>, values: &'a [Value]) -> (String, Vec<(String, &'a Value)>) {
    let scalar_fields = values
        .iter()
        .filter_map(Value::as_object)
        .flat_map(Map::iter)
        .filter(|(_, value)| !is_nested(value))
        .map(|(field, _)| field)
        .collect::<BTreeSet<_>>();
    let mut table = new_table();
    let mut header = vec!["INDEX"];
    header.extend(scalar_fields.iter().map(|field| field.as_str()));
    table.set_header(header);
    let mut nested_values = Vec::new();

    for (index, value) in values.iter().enumerate() {
        let object = value.as_object().expect("object list only contains objects");
        let mut row = vec![index.to_string()];
        row.extend(
            scalar_fields
                .iter()
                .map(|field| object.get(*field).map_or_else(|| "-".to_string(), scalar_to_string)),
        );
        table.add_row(row);

        for (field, value) in object.iter().filter(|(_, value)| is_nested(value)) {
            nested_values.push((indexed_child_path(label, index, field), value));
        }
    }

    (table.to_string(), nested_values)
}

fn render_value_list<'a>(label: Option<&str>, values: &'a [Value]) -> (String, Vec<(String, &'a Value)>) {
    let mut table = new_table();
    table.set_header(["VALUE"]);
    let mut nested_values = Vec::new();

    for (index, value) in values.iter().enumerate() {
        if is_nested(value) {
            table.add_row([child_marker(value)]);
            nested_values.push((indexed_path(label, index), value));
        } else {
            table.add_row([scalar_to_string(value)]);
        }
    }

    (table.to_string(), nested_values)
}

fn new_table() -> Table {
    Table::new()
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => "-".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => child_marker(value).to_string(),
    }
}

fn is_nested(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Object(_))
}

fn child_marker(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "(see list below)",
        Value::Object(_) => "(see details below)",
        _ => unreachable!("child marker is only used for nested values"),
    }
}

fn with_label(label: Option<&str>, rendered: String) -> String {
    label.map_or(rendered.clone(), |label| format!("{label}\n{rendered}"))
}

fn child_path(parent: Option<&str>, child: &str) -> String {
    parent.map_or_else(|| child.to_string(), |parent| format!("{parent}.{child}"))
}

fn indexed_path(parent: Option<&str>, index: usize) -> String {
    parent.map_or_else(|| format!("[{index}]"), |parent| format!("{parent}[{index}]"))
}

fn indexed_child_path(parent: Option<&str>, index: usize, child: &str) -> String {
    format!("{}.{}", indexed_path(parent, index), child)
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
            {"keyid": 2, "packet_key": "second"},
        ])));
    }

    #[test]
    fn renders_nested_sections_and_empty_lists() {
        insta::assert_snapshot!(render(&json!({
            "source": {
                "chain_key": "0x1234",
                "multi_addresses": ["/ip4/127.0.0.1/tcp/9091"],
            },
            "channels": [
                {
                    "channel": {"status": "OPEN", "ticket_index": "0"},
                    "destination": {"peer_id": "12D3KooWFullPeerId"},
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
