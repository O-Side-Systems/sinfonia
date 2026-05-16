//! WORKFLOW.md parsing (spec §5.2).

use crate::errors::{Error, Result};
use serde_yaml::Value as YamlValue;
use std::path::{Path, PathBuf};

/// Parsed workflow file: front matter + prompt body. §5.2.
#[derive(Debug, Clone)]
pub struct WorkflowDefinition {
    /// YAML front matter root, decoded to a JSON-like map.
    pub config: serde_json::Value,
    /// Prompt body, trimmed.
    pub prompt_template: String,
    /// Absolute path of the workflow file that produced this definition.
    pub path: PathBuf,
}

/// Read a workflow file from disk and parse it. §5.1, §5.2.
pub fn read_workflow_file(path: &Path) -> Result<WorkflowDefinition> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::MissingWorkflowFile(format!("{}: {}", path.display(), e)))?;
    let mut def = parse_workflow_str(&text)?;
    def.path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(def)
}

/// Parse workflow text. §5.2.
pub fn parse_workflow_str(text: &str) -> Result<WorkflowDefinition> {
    let (front_matter, body) = split_front_matter(text);

    let config = if let Some(fm) = front_matter {
        let yaml: YamlValue = serde_yaml::from_str(&fm)
            .map_err(|e| Error::WorkflowParseError(e.to_string()))?;
        match &yaml {
            YamlValue::Mapping(_) => yaml_to_json(&yaml)?,
            YamlValue::Null => serde_json::Value::Object(Default::default()),
            _ => return Err(Error::WorkflowFrontMatterNotMap),
        }
    } else {
        serde_json::Value::Object(Default::default())
    };

    Ok(WorkflowDefinition {
        config,
        prompt_template: body.trim().to_string(),
        path: PathBuf::new(),
    })
}

/// Returns `(Some(front_matter_yaml), body)` when the file starts with `---` and
/// the front matter terminator is found; otherwise `(None, whole_text)`.
fn split_front_matter(text: &str) -> (Option<String>, String) {
    let trimmed_start = text.trim_start_matches('\u{feff}'); // strip BOM
    if !trimmed_start.starts_with("---") {
        return (None, trimmed_start.to_string());
    }
    let mut lines = trimmed_start.lines();
    let first = lines.next().unwrap_or("");
    if first.trim() != "---" {
        return (None, trimmed_start.to_string());
    }

    let mut fm = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            fm.push_str(line);
            fm.push('\n');
        }
    }

    if !in_body {
        // No closing `---` — treat the whole file as body for resilience.
        return (None, trimmed_start.to_string());
    }
    (Some(fm), body)
}

fn yaml_to_json(value: &YamlValue) -> Result<serde_json::Value> {
    // serde_yaml::Value → serde_json::Value via the canonical re-serialize path.
    let s = serde_yaml::to_string(value).map_err(|e| Error::WorkflowParseError(e.to_string()))?;
    let v: serde_json::Value = serde_yaml::from_str(&s)
        .map_err(|e| Error::WorkflowParseError(e.to_string()))?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_front_matter() {
        let text = "---\ntracker:\n  kind: linear\n  project_slug: alpha\n---\nHello {{ issue.title }}\n";
        let def = parse_workflow_str(text).unwrap();
        assert!(def.prompt_template.starts_with("Hello"));
        assert_eq!(
            def.config["tracker"]["kind"].as_str(),
            Some("linear")
        );
    }

    #[test]
    fn parse_without_front_matter_uses_empty_map() {
        let def = parse_workflow_str("just a body").unwrap();
        assert!(def.config.is_object());
        assert_eq!(def.prompt_template, "just a body");
    }

    #[test]
    fn non_map_front_matter_errors() {
        let text = "---\n- a\n- b\n---\nbody\n";
        let err = parse_workflow_str(text).unwrap_err();
        matches!(err, Error::WorkflowFrontMatterNotMap);
    }
}
