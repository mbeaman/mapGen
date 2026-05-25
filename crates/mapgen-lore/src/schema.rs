//! The strict JSON contract the narrator must emit, and the parse that turns a
//! raw model response into a [`ChronicleDraft`] (pre-validation).

use serde::Deserialize;

/// The narrator's structured output, before NER validation. `references` are
/// event ids the prose drew on; `lacunae` are gaps it could not fill from the
/// supplied events.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ChronicleDraft {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub references: Vec<u32>,
    #[serde(default)]
    pub lacunae: Vec<String>,
}

/// The JSON shape handed to the model in the prompt (the SCHEMA block).
pub const SCHEMA_HINT: &str = r#"Respond with ONLY a JSON object, no prose around it:
{"title": "<short title>", "body": "<the chronicle>", "references": [<event ids you used>], "lacunae": ["<gaps you could not fill from the supplied events>"]}
Rules: name only entities present in the WORLD BIBLE or ENTITY CONTEXT; invent no
new persons, places, gods, artifacts, or dates; mark anything the events do not
tell you as "[lacuna]" in the body and list it in `lacunae`. `references` must be
ids drawn from the SUPPLIED EVENTS."#;

impl ChronicleDraft {
    /// Parse a model response into a draft. Tolerates leading/trailing prose by
    /// extracting the first balanced top-level `{...}` object — real models
    /// occasionally wrap JSON in commentary or code fences.
    pub fn from_response(text: &str) -> anyhow::Result<ChronicleDraft> {
        let json = extract_json_object(text)
            .ok_or_else(|| anyhow::anyhow!("no JSON object found in narrator response"))?;
        Ok(serde_json::from_str(json)?)
    }
}

/// Slice out the first balanced top-level `{...}` (brace-depth scan, skipping
/// braces inside string literals). Returns the substring including the braces.
fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clean_object() {
        let d = ChronicleDraft::from_response(
            r#"{"title":"T","body":"B","references":[1,2],"lacunae":[]}"#,
        )
        .unwrap();
        assert_eq!(d.title, "T");
        assert_eq!(d.references, vec![1, 2]);
    }

    #[test]
    fn tolerates_wrapping_prose_and_code_fences() {
        let d = ChronicleDraft::from_response(
            "Here is the chronicle:\n```json\n{\"title\":\"T\",\"body\":\"a } brace in text\"}\n```\nDone.",
        )
        .unwrap();
        assert_eq!(d.body, "a } brace in text");
        assert!(d.references.is_empty());
    }

    #[test]
    fn errors_when_no_object_present() {
        assert!(ChronicleDraft::from_response("no json here").is_err());
    }
}
