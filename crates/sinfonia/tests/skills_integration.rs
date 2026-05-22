//! Phase 5 §5.2 — skill manifest + template integration tests.
//!
//! For each skill folder under `skills/`:
//! 1. Parse `SKILL.md` (YAML front matter + body) and assert it has the
//!    required `name`, `description`, `version` keys.
//! 2. Walk `templates/*.liquid` and assert each one parses under the same
//!    Liquid configuration the daemon uses.
//! 3. Re-assert the §8 box-2 grep invariant — no unguarded
//!    `{{ issue.fields.* }}` references in any state-machine prompt.
//!
//! These tests catch the contract drift that's hardest to see at review
//! time: a missing front-matter key, a typo in a Liquid template, or a
//! prompt that lost its `| default:` guard.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn skills_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `crates/sinfonia`; skills/ is two up.
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("..").join("..").join("skills")
}

fn expected_skills() -> HashSet<&'static str> {
    HashSet::from([
        "setup-workflow",
        "setup-bridge",
        "setup-state-machine",
        "setup-telemetry",
        "setup-agent-backend",
        "migrate-from-symphony",
    ])
}

#[test]
fn all_six_skills_are_present() {
    // Plan-doc §2 commits to six skills shipped at v1.0. This test pins the
    // set so an accidental delete (or a typo in a folder name) fails CI
    // rather than silently dropping a skill from the catalog.
    let dir = skills_dir();
    let mut found = HashSet::new();
    for entry in std::fs::read_dir(&dir).expect("skills/ readable") {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let name = entry.file_name().into_string().unwrap();
            found.insert(name);
        }
    }
    let expected: HashSet<String> = expected_skills().into_iter().map(String::from).collect();
    assert_eq!(
        found, expected,
        "skills directory must contain exactly the six v1.0 skills"
    );
}

#[test]
fn every_skill_md_has_required_front_matter() {
    // Phase 5 §5.2 #1: front matter must carry `name`, `description`,
    // `version`. We parse via the same `parse_workflow_str` the daemon uses
    // for WORKFLOW.md — the two formats are identical (YAML front matter +
    // body), so a single parser covers both.
    for skill_name in expected_skills() {
        let skill_md = skills_dir().join(skill_name).join("SKILL.md");
        let text = std::fs::read_to_string(&skill_md)
            .unwrap_or_else(|e| panic!("read {skill_md:?}: {e}"));
        let def = sinfonia::config::parse_workflow_str(&text)
            .unwrap_or_else(|e| panic!("{skill_name} SKILL.md parse: {e}"));

        // The front-matter root should carry exactly the three required
        // keys (and any optional extras the skill author wants).
        let name_v = def.config.get("name").and_then(|v| v.as_str());
        let desc_v = def.config.get("description").and_then(|v| v.as_str());
        let ver_v = def.config.get("version").and_then(|v| v.as_str());

        assert_eq!(
            name_v,
            Some(skill_name),
            "{skill_name}: front-matter `name` must match folder name"
        );
        assert!(
            desc_v.map_or(false, |s| !s.trim().is_empty()),
            "{skill_name}: `description` must be a non-empty string"
        );
        assert!(
            ver_v.map_or(false, |s| !s.trim().is_empty()),
            "{skill_name}: `version` must be a non-empty string"
        );
        // The SKILL.md body must not be empty — an AI tool needs prose to
        // follow. (Skills with zero procedural content are useless.)
        assert!(
            !def.prompt_template.trim().is_empty(),
            "{skill_name}: SKILL.md body must be non-empty"
        );
    }
}

#[test]
fn every_liquid_template_parses() {
    // Phase 5 §5.2 #2 (scoped): every Liquid template in `skills/*/templates/`
    // must parse under the same `liquid` builder the daemon uses. This is a
    // syntax check — it does NOT prove the templates render correctly
    // against any specific context (that's the job of each skill's own
    // golden-output tests when AI tools run them).
    //
    // We don't try to render here because Liquid's strict-variable mode
    // would fail on any free variable the skill renderer would supply at
    // runtime (`tracker.kind`, `agent.provider`, etc.). Parse-only is a
    // tighter test of *template-author error* than render would be.
    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .expect("Liquid parser builds");

    for skill_name in expected_skills() {
        let tdir = skills_dir().join(skill_name).join("templates");
        if !tdir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&tdir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("liquid") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            parser
                .parse(&text)
                .unwrap_or_else(|e| panic!("{}: liquid parse: {e}", path.display()));
        }
    }
}

#[test]
fn state_machine_prompts_have_no_unguarded_issue_fields() {
    // Phase 5 §8 box 2 — the strict-Liquid contract.
    //
    // Every `{{ issue.fields.* }}` reference in a state-machine prompt
    // template MUST be followed by `| default:` so a human dragging a
    // ticket into Needs Fixes (without any prior bridge run) doesn't
    // crash the daemon's render_prompt path.
    //
    // Regex: `{{` then any non-`}` chars containing `issue.fields.`,
    // followed by any non-`|` chars, then `}}`. A match means the
    // reference has no pipe filter at all — that's an unguarded
    // reference. (References with `| default:` will have `|` between
    // `issue.fields.` and `}}` and won't match.)
    let re =
        regex::Regex::new(r"\{\{[^}]*issue\.fields\.[^|]*\}\}").expect("regex compiles");

    let tdir = skills_dir().join("setup-state-machine").join("templates");
    for entry in std::fs::read_dir(&tdir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("liquid") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if let Some(m) = re.find(&text) {
            panic!(
                "{}: unguarded `issue.fields.*` reference at byte {}: {:?}\n\
                 Every `{{{{ issue.fields.X }}}}` reference must be followed \
                 by `| default: \"...\"` so absent fields don't crash strict Liquid.",
                path.display(),
                m.start(),
                m.as_str(),
            );
        }
    }
}

#[test]
fn validators_are_executable() {
    // Validators are shell scripts called by the skill runner. They must be
    // chmod +x or the skill will fail at "validators/check-workflow.sh: not
    // executable" — a confusing error that points at the operator's
    // environment, not the actual missing-bit. Pin the bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for skill_name in expected_skills() {
            let vdir = skills_dir().join(skill_name).join("validators");
            if !vdir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&vdir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("sh") {
                    continue;
                }
                let mode = std::fs::metadata(&path).unwrap().permissions().mode();
                assert!(
                    mode & 0o111 != 0,
                    "{}: validator script must be executable (current mode: {:o})",
                    path.display(),
                    mode & 0o777
                );
            }
        }
    }
}

#[test]
fn workflow_template_round_trips_through_check() {
    // Render-time golden: take the `setup-workflow` template, plug in a
    // realistic Liquid context, and assert the rendered WORKFLOW.md parses
    // under `sinfonia --check`'s schema layer (i.e. doesn't lose the front
    // matter shape during template variable substitution).
    let tpl_path = skills_dir()
        .join("setup-workflow")
        .join("templates")
        .join("workflow.md.liquid");
    let tpl_src = std::fs::read_to_string(&tpl_path).unwrap();

    let parser = liquid::ParserBuilder::with_stdlib().build().unwrap();
    let template = parser.parse(&tpl_src).expect("template parses");

    let context = liquid::object!({
        "tracker": {
            "kind": "linear",
            "api_key_env": "LINEAR_API_KEY",
            "project_slug": "my-project",
            "active_states": ["Todo", "In Progress"],
            "terminal_states": ["Done", "Cancelled"],
        },
        "agent": {
            "provider": "anthropic",
        },
        "llm": {
            "api_key_env": "ANTHROPIC_API_KEY",
            "model": "claude-opus-4-7",
        },
        "workspace": {
            "root": "./workspaces",
        },
        "hooks": {
            "after_create": "npm install",
            "before_run": "npm test --silent",
        },
    });

    let rendered = template.render(&context).expect("render succeeds");

    // Set the env vars the rendered WORKFLOW.md references; `parse_workflow_str`
    // followed by `validate_for_dispatch` is the same path `sinfonia --check`
    // takes when the operator runs it.
    std::env::set_var("LINEAR_API_KEY", "test-key");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let def = sinfonia::config::parse_workflow_str(&rendered)
        .expect("rendered workflow parses");
    let cfg = sinfonia::config::ServiceConfig::from_workflow(&def)
        .expect("rendered workflow has valid schema");
    cfg.validate_for_dispatch()
        .expect("rendered workflow passes validate_for_dispatch");
}
