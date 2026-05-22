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

    // Serialize the Issue, then patch the `fields` sub-object so that every
    // well-known sinfonia_* custom-field key has an explicit nil entry when
    // the bridge hasn't populated it yet. Strict-mode Liquid errors on
    // "Unknown index" *before* the `| default:` filter would have a chance
    // to fire; pre-seeding the well-known keys as nil makes the
    // `{{ issue.fields.X | default: "…" }}` pattern that the
    // `setup-state-machine` skill emits actually work. See `docs/SPEC.md`
    // §11.6.4.
    let mut issue_value = liquid::model::to_value(issue)
        .map_err(|e| Error::TemplateRenderError(format!("issue serialize: {e}")))?;
    if let Some(obj) = issue_value.as_object_mut() {
        // Pull / build the `fields` sub-object we just emitted.
        // `into_object()` returns None if the value wasn't an object, and
        // we treat "field absent or not an object" the same way: start from
        // an empty Object and seed the well-known keys as nil.
        let mut fields_obj = obj
            .remove("fields")
            .and_then(|v| v.into_object())
            .unwrap_or_default();
        for key in sinfonia_tracker::custom_fields::WELL_KNOWN_FIELDS {
            fields_obj
                .entry(liquid::model::KString::from_static(key))
                .or_insert(liquid::model::Value::Nil);
        }
        obj.insert("fields".into(), liquid::model::Value::Object(fields_obj));
    }
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
            fields: Default::default(),
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

    // ---- H-1 round-trip: bridge custom fields in the prompt scope ----------
    //
    // The whole feedback-loop story depends on the agent's prompt template
    // being able to read what the bridge wrote into `Issue.fields`. The two
    // tests below pin that contract: the populated case must render the
    // value verbatim, and the unset case must fall through to the
    // `| default:` filter (rather than failing strict-mode validation).
    //
    // The setup-state-machine skill (Phase 5) enforces `| default:` on
    // every generated reference; these tests ensure the underlying
    // mechanism it relies on is present.

    #[test]
    fn issue_fields_render_when_populated() {
        let mut iss = sample_issue();
        iss.fields.insert(
            "sinfonia_last_ci_failure".into(),
            sinfonia_tracker::CustomFieldValue::text(
                "playwright e2e: login_flow.spec.ts timed out",
            ),
        );
        iss.fields.insert(
            "sinfonia_attempt_count".into(),
            sinfonia_tracker::CustomFieldValue::Number(3.0),
        );

        let s = render_prompt(
            "attempt {{ issue.fields.sinfonia_attempt_count }} / failure: {{ issue.fields.sinfonia_last_ci_failure }}",
            &iss,
            None,
        )
        .unwrap();
        assert!(
            s.contains("attempt 3"),
            "number variant should render as a bare number, got: {s}"
        );
        assert!(
            s.contains("playwright e2e: login_flow.spec.ts timed out"),
            "string variant should render verbatim, got: {s}"
        );
        // Specifically NOT the tagged-JSON form that a derived Serialize
        // would have produced — guards against an implementer accidentally
        // adding `#[derive(Serialize)]` to CustomFieldValue.
        assert!(
            !s.contains("\"Number\""),
            "must not emit tagged JSON, got: {s}"
        );
        assert!(
            !s.contains("\"String\""),
            "must not emit tagged JSON, got: {s}"
        );
    }

    #[test]
    fn issue_fields_default_filter_handles_absent_field() {
        // Mirrors a Phase 5 setup-state-machine template against a ticket
        // a human dragged into "Needs Fixes" without any prior CI run.
        // `sinfonia_last_ci_failure` is unset. Without `| default:` this
        // would fail strict Liquid; with `| default:` it should render the
        // fallback text.
        let iss = sample_issue();
        let s = render_prompt(
            "Last failure: {{ issue.fields.sinfonia_last_ci_failure | default: \"(none recorded)\" }}",
            &iss,
            None,
        )
        .unwrap();
        assert!(
            s.contains("(none recorded)"),
            "default filter should have fired, got: {s}"
        );
    }
}
