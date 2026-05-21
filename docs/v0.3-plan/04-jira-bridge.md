# Phase 4 — Jira support for the bridge

**Target:** v0.3.0
**Scope:** Extend `sinfonia-bridge` (Linear-only after Phase 1) to also drive Jira Cloud and Jira self-hosted via the existing tracker abstraction.
**Estimated size:** ~200 LOC + ~150 LOC of tests + ~150 LOC of docs.
**Depends on:** Phase 1 (bridge exists with custom-fields trait), Phase 2 unrelated. Phase 3's budget custom fields are also Jira-aware.
**Unblocks:** Phase 5's `setup-bridge` skill prompts for Jira as a tracker option.

This is the smallest phase by code volume. Most of the work was already done in Phase 1's `IssueTracker` trait extension; here we land the Jira side of that extension.

---

## 1. What Phase 1 left unfinished for Jira

The trait extensions in `01-bridge-mvp.md` §2 added five methods:

```rust
async fn transition_issue(&self, id: &str, target_state: &str) -> Result<()>;
async fn read_custom_field(&self, id: &str, key: &str) -> Result<CustomFieldValue>;
async fn write_custom_field(&self, id: &str, key: &str, value: CustomFieldValue) -> Result<()>;
async fn ensure_custom_field(&self, schema: &CustomFieldSchema) -> Result<()>;
async fn post_comment(&self, id: &str, body: &str) -> Result<()>;
```

In Phase 1 the Linear implementation was the comment-marker-payload approach. The Jira implementation returned `Err(NotImplemented)`. Phase 4 fills in the Jira side.

The Phase 1 bridge config gates Jira off with:

> `tracker.kind` is `linear` (Phase 1) — `jira` errors with "deferred to Phase 4"

In Phase 4 we remove that gate.

---

## 2. Why Jira's harder than Linear

Three nontrivial differences:

1. **Custom fields are real first-class entities.** They have IDs (`customfield_10037`), screen schemes that gate which fields appear on which screens, and field configurations that gate which fields are required/optional/readable. The bridge has to either create the fields (cheap) and bind them to the right screens (annoying), or document a manual setup step.

2. **State transitions go through transition IDs**, not state names. Jira's REST API requires `POST /rest/api/3/issue/{key}/transitions` with a transition ID. The bridge has to look up "which transition from current state leads to target state."

3. **Comment formatting is ADF (Atlassian Document Format)**, not Markdown. ADF is a JSON tree. Failure-comment templates are authored in Markdown by users; we convert to ADF before posting.

None of these are theoretically hard; they all just take measurable work.

---

## 3. Implementation

All code lands in `crates/sinfonia-tracker/src/jira.rs`. The existing `JiraTracker` (Phase 1, line counts in original survey ~426 LOC) gains the five new methods.

### 3.1 `ensure_custom_field`

```rust
async fn ensure_custom_field(&self, schema: &CustomFieldSchema) -> Result<()> {
    let fields = self.get(&format!("/rest/api/3/field/search?query={}", schema.key)).await?;
    if fields.values.iter().any(|f| f.name == schema.display_name) {
        return Ok(());
    }
    let body = json!({
        "name": schema.display_name,
        "description": schema.description.clone().unwrap_or_default(),
        "type": jira_field_type(schema.kind),
        "searcherKey": jira_searcher_key(schema.kind),
    });
    self.post("/rest/api/3/field", &body).await?;
    // Note: the field is created but not bound to any screen yet — see §3.4.
    Ok(())
}

fn jira_field_type(kind: CustomFieldKind) -> &'static str {
    match kind {
        CustomFieldKind::Number  => "com.atlassian.jira.plugin.system.customfieldtypes:float",
        CustomFieldKind::Decimal => "com.atlassian.jira.plugin.system.customfieldtypes:float",
        CustomFieldKind::LongText => "com.atlassian.jira.plugin.system.customfieldtypes:textarea",
        CustomFieldKind::Url     => "com.atlassian.jira.plugin.system.customfieldtypes:url",
    }
}
```

The bridge caches the resolved `key → customfield_NNNNN` mapping after first read so subsequent calls don't refetch.

### 3.2 `read_custom_field` / `write_custom_field`

```rust
async fn read_custom_field(&self, id: &str, key: &str) -> Result<CustomFieldValue> {
    let resolved = self.resolve_field_id(key).await?;             // cached
    let issue = self.get(&format!("/rest/api/3/issue/{id}?fields={resolved}")).await?;
    Ok(parse_field_value(&issue.fields[&resolved]))
}

async fn write_custom_field(&self, id: &str, key: &str, value: CustomFieldValue) -> Result<()> {
    let resolved = self.resolve_field_id(key).await?;
    let payload = json!({ "fields": { resolved: serialize_field_value(value) } });
    self.put(&format!("/rest/api/3/issue/{id}"), &payload).await
}
```

`resolve_field_id` is a `tokio::sync::RwLock<HashMap<String, String>>` that survives the bridge process lifetime — Jira field IDs don't change mid-project.

### 3.3 `transition_issue`

```rust
async fn transition_issue(&self, id: &str, target_state: &str) -> Result<()> {
    let transitions = self.get(&format!("/rest/api/3/issue/{id}/transitions")).await?;
    let target = transitions.values.iter()
        .find(|t| t.to.name.eq_ignore_ascii_case(target_state))
        .ok_or_else(|| Error::no_transition_to(target_state, id))?;
    self.post(
        &format!("/rest/api/3/issue/{id}/transitions"),
        &json!({ "transition": { "id": target.id } }),
    ).await
}
```

Failure mode: no transition from the current state to the target. In Jira this means the workflow scheme doesn't allow the move. The error message includes the source state, target state, and a link to "Edit your workflow to add a transition from X to Y" — concrete and actionable.

### 3.4 The screen-scheme problem

Created custom fields are NOT automatically visible on the issue view or editable in screens. Jira admins have to bind them to a screen. We have three options:

| Option | Pros | Cons |
|---|---|---|
| **Bind via API at startup** | fully automated | requires admin permissions; failure mode if user lacks them is confusing |
| **Document manual setup; error if not bound** | predictable failures | one extra setup step |
| **Just write the field; let Jira return the value via API even when not on a screen** | works for any user with edit perms | the field is invisible in the UI, which is bad UX |

Choice: **bind via API at startup with a fallback to documentation.** If `ensure_custom_field` succeeds at creation but the subsequent screen-scheme bind fails with 403, the bridge logs:

```
WARN bridge: created custom field 'sinfonia_attempt_count' but cannot bind it to
     screens (HTTP 403). The field will work programmatically but won't be
     visible in the Jira UI. See docs/JIRA-SCREEN-SCHEME.md to bind manually.
```

`docs/JIRA-SCREEN-SCHEME.md` is a one-page guide that ships with this phase.

### 3.5 ADF conversion for comments

The bridge's failure-comment template (in `BRIDGE.md`) is Markdown. Jira's `POST /rest/api/3/issue/{key}/comment` expects ADF.

We use a small Markdown → ADF converter. Two options:

1. **Write our own** — a few hundred lines for the limited Markdown subset we generate (paragraphs, code blocks, lists, links).
2. **Use the `markdown-to-adf` crate** — exists on crates.io but unverified maturity. Spike before deciding.

Default: write our own, narrow-scope, in `crates/sinfonia-tracker/src/jira/adf.rs`. The template's Liquid output is a fixed shape (a few paragraphs, a code block of failed checks, a code block of log excerpt). A 200-line converter handles that shape with margin to spare.

A future enhancement could let users author the template directly in ADF if they care, but the marginal value is low.

### 3.6 Cloud vs self-hosted

The existing `JiraTracker` already supports both via the `endpoint:` config field. All five new methods are endpoint-agnostic — they use the same `/rest/api/3/...` paths and either Basic (Cloud) or PAT (self-hosted) auth that's already wired.

A small gotcha: self-hosted Jira often runs a different version of the REST API. We document the minimum supported version (Jira Server 9.x+ / Data Center 9.x+) and verify it with a `--self-test` check in Phase 5's `setup-bridge` skill.

---

## 4. Configuration

`BRIDGE.md` gains the Jira variant:

```yaml
---
tracker:
  kind: jira                                   # was: linear-only
  endpoint: https://yourorg.atlassian.net      # Cloud
  email: you@yourorg.com
  api_key: $JIRA_API_TOKEN
  project_slug: ENG                            # Jira project key, not numeric ID
---
```

The `setup-bridge` skill (Phase 5) prompts the user for these and asks "Cloud or self-hosted?" up front. For self-hosted, `email` is replaced with a PAT-only auth (Jira self-hosted's PATs don't require an email pair).

---

## 5. Test plan

### 5.1 Unit tests

| Module | What it covers |
|---|---|
| `jira::field_type_mapping` | `CustomFieldKind` → Jira type string. |
| `jira::resolve_field_id` | Cached lookup, cache miss path. |
| `jira::adf::tests` | Markdown → ADF for paragraphs, code blocks, lists, links. |
| `jira::transition_lookup` | Happy path, no-transition error. |

### 5.2 Integration tests

`tests/jira_bridge_e2e.rs` uses `wiremock` with the recorded shape of Jira's REST responses. The same scenarios from Phase 1 (§9.2 in `01-bridge-mvp.md`) re-run against the Jira mock to confirm parity with Linear.

Specifically the four "red CI runs" + "cap hit" scenarios — these are the spec-conformance acceptance tests for the bridge, and they have to pass against both tracker kinds.

### 5.3 Manual verification

Two real-world runs:

1. Jira Cloud sandbox + GitHub sandbox repo. Ticket goes red → counter increments → category routing → cap hit → blocked. Same shape as Phase 1's Linear verification.
2. Jira Server / Data Center if any team has one available. Lower priority — Cloud is the dominant deployment.

Results captured in `docs/v0.3-plan/04-jira-VERIFY.md`.

---

## 6. Dependencies

No new crates beyond what Phase 1 added. The Markdown→ADF converter is hand-rolled.

If we change our mind about hand-rolling, add:

```toml
markdown = "1"     # if needed for AST parsing
```

— and emit ADF from our own visitor. Decision deferred to implementation pass.

---

## 7. Open questions

1. **Per-project field creation.** Jira custom fields are global. If two Sinfonia bridges run against the same Jira instance for different projects, they share the same field. That's fine semantically — `sinfonia_attempt_count` means the same thing everywhere. Document that the field names are intentionally globally-scoped.

2. **API-version pinning.** Both Linear and Jira have versioned APIs. We pin to `/rest/api/3/` for Jira. Linear has no versioning at the GraphQL layer; we accept whatever Linear returns. Document that.

3. **Self-hosted authentication edge cases.** Some self-hosted instances use SSO with no token issuance. Bridge errors clearly if `email` and `api_key` are both unset; the `setup-bridge` skill catches this at config time.

4. **ADF gaps.** If a user writes a more elaborate `failure_comment_template` (tables, images), our converter won't handle it. Behavior: log a warning and emit the text as a plain paragraph. Document the supported subset.

---

## 8. Phase 4 deliverable checklist

- [ ] Five `IssueTracker` methods implemented in `crates/sinfonia-tracker/src/jira.rs`.
- [ ] Markdown → ADF converter in `crates/sinfonia-tracker/src/jira/adf.rs` with the supported-subset documented.
- [ ] Bridge config validation no longer rejects `tracker.kind: jira`.
- [ ] Screen-scheme binding attempt + clear error path + `docs/JIRA-SCREEN-SCHEME.md`.
- [ ] Unit tests per §5.1.
- [ ] Integration tests per §5.2 mirroring Phase 1's Linear scenarios.
- [ ] Manual verification recorded in `docs/v0.3-plan/04-jira-VERIFY.md`.
- [ ] `BRIDGE.example.md` updated with both `kind: linear` and `kind: jira` sections.
- [ ] `docs/SPEC.md` §11.6 reflects both tracker implementations.
- [ ] CHANGELOG entry.

Phase 4 ships independently of Phases 5-7.
