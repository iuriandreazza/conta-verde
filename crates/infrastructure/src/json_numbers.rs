use serde_json::Value;
use uuid::Uuid;

/// The marker a number is written with until the JSON is rendered: the exact decimal text goes into the body, never
/// a float ([ADR 0003]). The marker is unique to a body, so no text of a customer can be taken for one.
pub(crate) struct Numbers {
    marker: String,
}

impl Numbers {
    pub(crate) fn new() -> Self {
        Self {
            marker: format!("@@{}:", Uuid::new_v4().simple()),
        }
    }

    pub(crate) fn of(&self, number: impl std::fmt::Display) -> Value {
        Value::String(format!("{}{number}@@", self.marker))
    }

    pub(crate) fn render(&self, body: &Value) -> String {
        let text = body.to_string();
        let opening = format!("\"{}", self.marker);
        let mut rendered = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find(&opening) {
            rendered.push_str(&rest[..start]);
            let after = &rest[start + opening.len()..];
            // A number is digits, a point and a minus sign: it cannot hold the closing marker.
            let end = after.find("@@\"").unwrap_or(after.len());
            rendered.push_str(&after[..end]);
            rest = after.get(end + 3..).unwrap_or("");
        }
        rendered.push_str(rest);
        rendered
    }
}
