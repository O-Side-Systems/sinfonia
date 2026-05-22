# Binding `sinfonia_*` custom fields to Jira screens

**Audience:** Jira admins running the Sinfonia bridge against a Jira Cloud or
self-hosted (Server / Data Center) project. **Read time:** ~5 min.

When the bridge starts against a Jira project for the first time it creates
the seven `sinfonia_*` custom fields (`Sinfonia Attempt Count`,
`Sinfonia Last CI Failure`, …) and then attempts to bind them to a screen so
they appear in the Jira issue view. The bind step requires the **Jira
Administrators** group permission (or the Site Admin role on Cloud).

If the bridge logs a line that looks like this:

```
WARN tracker.jira: field=Sinfonia Attempt Count error=... created custom
     field but could not bind it to any screen — the field will work
     programmatically but won't be visible in the Jira UI. See
     docs/JIRA-SCREEN-SCHEME.md to bind manually.
```

— the field **was created and is fully writable via REST**, so the bridge's
feedback loop keeps working. What's missing is the screen binding that makes
the field show up in the Jira UI for human inspection. The rest of this
document walks through the one-time manual setup.

---

## Manual bind — Jira Cloud

1. **Settings → Issues → Custom fields.** Filter for `sinfonia_`. You should
   see the seven well-known fields the bridge created.
2. For each field, click the three-dot menu → **Associate to screens**.
3. Pick the **Default Screen** for the project (or the screen scheme your
   project uses — usually `<KEY>: Scrum Default Screen Scheme` or
   `<KEY>: Kanban Default Screen Scheme`).
4. Confirm. The field will now appear in the issue detail view.

You only need to bind these once per Jira project. Field IDs (`customfield_NNNNN`)
are global to the instance, so multiple bridges hitting the same Jira tenant
share the same fields — bind once, and every project sees them.

## Manual bind — Jira Server / Data Center

1. **Cog icon → Issues → Custom fields.** Filter for `sinfonia_`.
2. For each field, click the three-dot menu → **Screens**.
3. Tick the screens you want to expose the field on — typically the
   `<KEY>: Scrum Default Screen` and any custom screens the project uses.
4. Save.

---

## What the bridge actually checks

The bridge does NOT verify that any of these fields is screen-bound before
proceeding. It checks only:

1. The field **exists** — by listing all fields via
   `GET /rest/api/3/field` and matching the bridge's stable display name
   (`Sinfonia Attempt Count` etc.) to the field's `name`.
2. The field **is writable** — by issuing `PUT /rest/api/3/issue/{id}`
   with the `customfield_NNNNN` in the `fields` map.

This means a misconfigured screen scheme **never blocks the feedback loop**.
The cost is purely UI visibility for humans inspecting the ticket.

## Why we don't fully automate it

Atlassian's screen-scheme model is multi-tier (screen ⇢ screen scheme ⇢
issue type screen scheme ⇢ project mapping). Walking that tree
programmatically requires
[`/rest/api/3/screens`](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-screens/),
[`/rest/api/3/screenscheme`](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-screen-schemes/),
and
[`/rest/api/3/issuetypescreenscheme`](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-type-screen-schemes/)
calls, all gated on the global Admin permission. We do a best-effort first
attempt (one screen, first tab) and then defer to this manual flow rather
than failing the bridge boot in the common case where the bridge token
lacks site-admin perms.

## Reference: the field set the bridge owns

| Display name (Jira)            | Stable key                          | Type     | Written by      |
|--------------------------------|--------------------------------------|----------|------------------|
| Sinfonia Attempt Count         | `sinfonia_attempt_count`             | Number   | feedback loop    |
| Sinfonia Last CI Failure       | `sinfonia_last_ci_failure`           | LongText | feedback loop    |
| Sinfonia Failure Category      | `sinfonia_failure_category`          | LongText | feedback loop    |
| Sinfonia Max Attempts          | `sinfonia_max_attempts`              | Number   | operator (manual)|
| Sinfonia Tokens Consumed       | `sinfonia_tokens_consumed`           | Number   | telemetry phase  |
| Sinfonia Cost Consumed USD     | `sinfonia_cost_consumed_usd`         | LongText | telemetry phase  |
| Sinfonia Max Cost USD          | `sinfonia_max_cost_usd`              | LongText | operator (manual)|
| Sinfonia Budget Exhausted At   | `sinfonia_budget_exhausted_at`       | LongText | budget enforcer  |

Operators MAY pre-create these fields with their preferred display names; the
bridge only matches by display name (case-insensitive). If you do this, set
the field type to one of the equivalents documented in plan §3.1
(`...customfieldtypes:float` for Number, `...:textarea` for LongText).
