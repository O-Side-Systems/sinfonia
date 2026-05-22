# Phase 4 — Jira bridge support — VERIFY

This document captures the manual-verification status of Phase 4 and the
deliberate scope deferrals made during implementation. It mirrors the
structure of `02-opencode-VERIFY.md` and `03-telemetry-VERIFY.md` — a
sibling artifact that holds what `gsd-plan-checker` would have caught
after-the-fact.

**Status:** Code + unit + wiremock-integration tests complete (210 tests
passing on `v0.3-phase-4-jira-bridge`). Manual verification against a
real Atlassian Cloud sandbox is pending before `v0.3.0-alpha.x` ships.

---

## 1. Deltas vs. the plan-doc proposed surface

The Phase 2/3 lesson generalized: any time the plan doc references a
vendor-API endpoint shape or a wire-format dialect, verify it before
writing code. Phase 4 turned up two small refinements:

### 1.1 Field discovery endpoint — `/field` not `/field/search`

The plan doc (`04-jira-bridge.md` §3.1) proposed:

```rust
let fields = self.get(&format!("/rest/api/3/field/search?query={}", schema.key)).await?;
if fields.values.iter().any(|f| f.name == schema.display_name) { ... }
```

The actual `/rest/api/3/field/search` endpoint:

- Requires the `manage:jira-configuration` OAuth 2.0 scope, which the
  bridge would have to ask for at config time and which most operators
  can't grant.
- Returns the paged shape `{values:[…], total, isLast}` — not great if
  the bridge wants the full list.

The plain `/rest/api/3/field` (GET) endpoint, which the bridge actually
uses, has neither problem: it's part of the standard read-fields scope
that every Jira API token grants by default, and it returns a flat
array of every field on the instance. The bridge fetches it once per
bridge-key on first read and caches the resolved `customfield_NNNNN` in
an `Arc<RwLock<HashMap>>` for the process lifetime (plan §3.2 — the
caching design itself was correct, only the discovery URL changed).

### 1.2 Field identification is by display name, not by stable bridge key

The plan doc implies a query parameter against the bridge's stable key
(`sinfonia_attempt_count`) — but Jira's `/rest/api/3/field` payload
doesn't expose any "stable key" concept the bridge can use. Custom
fields have only:

- `id` — `customfield_10037`, allocated by Jira at creation time.
- `name` — the human-readable display name (e.g.
  `"Sinfonia Attempt Count"`).
- `schema` — type info; not useful for identification.

So the bridge identifies its fields by mapping each well-known stable
key to a stable display name via the
`jira::display_name_for_key(key) -> String` helper. The full mapping
ships in the source (`crates/sinfonia-tracker/src/jira.rs` `display_name_for_key`)
and is documented in `docs/JIRA-SCREEN-SCHEME.md` for operators who want
to pre-create the fields.

---

## 2. Manual verification matrix (pending external Jira sandbox)

The wiremock integration tests in
`crates/sinfonia-tracker/tests/jira_wiremock.rs` exercise the actual HTTP
path of every write method (six scenarios; all green). The matrix below
records what still needs to run against a real Atlassian instance before
the milestone ships.

| # | Scenario | Test type | Status |
|---|---|---|---|
| V-1 | Bridge boot against a fresh Jira Cloud project — verify the seven `sinfonia_*` custom fields are created and (where the token has admin perms) bound to a screen | Manual | **Pending** |
| V-2 | Bridge boot against an existing Jira project where the fields already exist — verify `ensure_custom_field` is idempotent (no duplicate creates) | Manual | **Pending**; covered by `ensure_custom_field_is_idempotent_when_field_exists` in wiremock |
| V-3 | One red CI run → `transition_issue("ENG-7", "Needs Fixes")` + `post_comment` lands the ADF body that renders correctly in the Jira UI | Manual | **Pending**; ADF wire shape covered by `post_comment_converts_markdown_to_adf` in wiremock |
| V-4 | Cap hit → `transition_issue("ENG-7", "Blocked - Human Review")` + `write_custom_field("sinfonia_budget_exhausted_at", …)` | Manual | **Pending**; per-method paths green in wiremock |
| V-5 | Token without admin perms → bind step warn-logs, field remains REST-writable, feedback loop proceeds | Manual | **Pending** |
| V-6 | Self-hosted Jira Server / Data Center 9.x — same scenarios as Cloud | Manual | **Lower priority** per plan §5.3 (Cloud is the dominant deployment) |

A `setup-bridge` `--self-test` extension in Phase 5 will fold V-1, V-2,
V-5 into an automated probe; we explicitly defer that to Phase 5
rather than build a one-off probe here.

---

## 3. Intentional scope deferrals

### 3.1 Full bridge-feedback-loop e2e against Jira mock

The plan doc (§5.2) proposed mirroring the Phase 1 Linear bridge_e2e
harness against a Jira mock — "the four 'red CI runs' + 'cap hit'
scenarios". `crates/sinfonia-bridge/tests/bridge_e2e.rs` is ~1400 LOC
of plumbing built around Linear GraphQL specifics, and re-templating it
for Jira's REST API would balloon Phase 4 well past its ~200 LOC
implementation + ~150 LOC test budget for no incremental safety: the
feedback-loop logic is tracker-agnostic and is already covered once by
the Linear e2e harness, and the Jira-specific wire shapes are exercised
by `tests/jira_wiremock.rs` (six scenarios driving every write method
through a real HTTP path).

If a future phase introduces tracker-dependent feedback-loop logic
(e.g. Jira-specific field-vs-comment fallbacks), revisit the decision.

### 3.2 Screen-scheme bind beyond the first tab

The bridge attempts a best-effort bind to the first tab of one screen
on `ensure_custom_field` and otherwise points operators at
`docs/JIRA-SCREEN-SCHEME.md`. A fully-automated bind across all of an
instance's screen schemes would require walking the four-tier
screen/scheme/issue-type-screen-scheme/project-mapping graph (plan §3.4)
and is gated on Admin permissions the bridge token usually doesn't have.
Cost/benefit is unfavorable; deferred indefinitely.

### 3.3 ADF beyond the supported subset

The Markdown → ADF converter in `crates/sinfonia-tracker/src/jira_adf.rs`
handles paragraphs, fenced code blocks, bullet/ordered lists, and inline
strong/em/code/link — the shape the default
`feedback_loop.failure_comment_template` emits. Tables, images,
blockquotes, headings, and HTML fall through to plain paragraphs (plan
§7 open question #4). The fall-through is deterministic and won't crash;
operators who care about richer rendering can author the template
directly in ADF JSON in a future enhancement.

---

## 4. Test baseline on `v0.3-phase-4-jira-bridge`

```
$ cargo test --workspace
test result: ok. 47 passed (sinfonia)
test result: ok. 13 passed (sinfonia-cli)
test result: ok. 110 passed (sinfonia-bridge unit)
test result: ok. 9 passed   (sinfonia-bridge bridge_e2e)
test result: ok. 25 passed  (sinfonia-tracker unit; +8 jira, +9 jira_adf)
test result: ok. 6 passed   (sinfonia-tracker jira_wiremock — new in P4)
```

210 tests, 0 failures. No new clippy regressions vs the `main` baseline.
