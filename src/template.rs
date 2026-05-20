//! Strict prompt rendering (spec §12).

use crate::domain::Issue;
use crate::errors::{Error, Result};

const FALLBACK_PROMPT: &str = "You are working on an issue from the tracker.";

/// Render the workflow prompt template with `issue` and `attempt` bound.
/// §12.2: strict variable + filter checking.
pub fn render_prompt(template: &str, issue: &Issue, attempt: Option<u32>) -> Result<String> {
    let text = template.trim();
    let template_src = if text.is_empty() {
        FALLBACK_PROMPT
    } else {
        text
    };

    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| Error::TemplateParseError(e.to_string()))?;
    let tpl = parser
        .parse(template_src)
        .map_err(|e| Error::TemplateParseError(e.to_string()))?;

    let mut object = liquid::Object::new();
    let issue_value = liquid::model::to_value(issue)
        .map_err(|e| Error::TemplateRenderError(format!("issue serialize: {e}")))?;
    object.insert("issue".into(), issue_value);
    object.insert(
        "attempt".into(),
        match attempt {
            None => liquid::model::Value::Nil,
            Some(n) => liquid::model::Value::scalar(n as i64),
        },
    );

    tpl.render(&object)
        .map_err(|e| Error::TemplateRenderError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Issue;

    fn sample_issue() -> Issue {
        Issue {
            id: "1".into(),
            identifier: "ABC-1".into(),
            title: "Fix bug".into(),
            description: Some("details".into()),
            priority: Some(2),
            state: "Todo".into(),
            branch_name: None,
            url: None,
            labels: vec!["bug".into()],
            blocked_by: vec![],
            children: vec![],
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn renders_issue_and_attempt() {
        let s = render_prompt(
            "Issue {{ issue.identifier }} (attempt {{ attempt }})",
            &sample_issue(),
            Some(3),
        )
        .unwrap();
        assert!(s.contains("ABC-1"));
        assert!(s.contains("attempt 3"));
    }

    #[test]
    fn children_are_exposed_to_template() {
        let mut issue = sample_issue();
        issue.children = vec![
            crate::domain::ChildRef {
                id: Some("c1".into()),
                identifier: Some("ABC-2".into()),
                state: "Done".into(),
            },
            crate::domain::ChildRef {
                id: Some("c2".into()),
                identifier: Some("ABC-3".into()),
                state: "Cancelled".into(),
            },
        ];
        let s = render_prompt(
            "Parent has {{ issue.children | size }} children: {% for c in issue.children %}{{ c.identifier }}={{ c.state }} {% endfor %}",
            &issue,
            None,
        )
        .unwrap();
        assert!(s.contains("2 children"));
        assert!(s.contains("ABC-2=Done"));
        assert!(s.contains("ABC-3=Cancelled"));
    }

    #[test]
    fn for_loop_variable_is_out_of_scope_after_endfor() {
        // Regression: an earlier WORKFLOW.md referenced `{{ c.identifier }}`
        // outside its `{% for c in issue.children %}` loop. Strict Liquid
        // rejects that with "Unknown variable c", which surfaced as
        // template_render_error at runtime. This test pins that behavior so
        // a similar prompt regression fails fast in CI.
        let mut issue = sample_issue();
        issue.children = vec![crate::domain::ChildRef {
            id: Some("c1".into()),
            identifier: Some("ABC-2".into()),
            state: "Done".into(),
        }];
        let bad = "{% for c in issue.children %}{{ c.identifier }}{% endfor %}\n{{ c.identifier }}";
        assert!(render_prompt(bad, &issue, None).is_err());
    }

    #[test]
    fn strict_mode_rejects_unknown_filter() {
        let err = render_prompt("{{ issue.title | bogusfilter }}", &sample_issue(), None)
            .unwrap_err();
        matches!(err, Error::TemplateParseError(_) | Error::TemplateRenderError(_));
    }
}
