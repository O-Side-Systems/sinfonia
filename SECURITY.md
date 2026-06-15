# Security Policy

## Reporting a vulnerability

**Please do not file public GitHub issues for vulnerabilities.** Use one of the channels below instead so we can coordinate a fix before disclosure.

- **GitHub Security Advisories** — preferred: open a private advisory at https://github.com/O-Side-Systems/sinfonia/security/advisories/new
- **Email** — `info@oside.systems` (replace with the real address before publishing)

Include:

1. A clear description of the issue and its impact.
2. Steps to reproduce, ideally with a minimal `WORKFLOW.md`, tracker fixture, and command line.
3. The Sinfonia version (`sinfonia --version` if available, or commit SHA).
4. Whether you'd like to be credited in the advisory, and how.

We aim to acknowledge new reports within 3 business days and to ship a fix or mitigation within 30 days for high-severity issues. Lower-severity findings are bundled into the next minor release.

## Supported versions

Only the latest minor release receives security fixes. Older versions are best-effort.

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅        |
| < 0.1   | ❌        |

## Threat model summary

Sinfonia is a daemon that runs untrusted coding agents against your workspaces. The trust boundary is:

| Trusted                                                                  | Not trusted                                                                                    |
|--------------------------------------------------------------------------|------------------------------------------------------------------------------------------------|
| `WORKFLOW.md` and the host system it runs on                             | LLM output (treated as data — but agent tool calls execute under the daemon's user)            |
| API keys configured in env or `WORKFLOW.md`                              | Tracker issue contents (titles, descriptions, comments) — these become prompt input            |
| Operator-installed `claude` / `codex` CLIs and their credentials         | The runtime workspace contents an agent produces                                               |

Practical implications:

- **Hooks (`after_create`, `before_run`, etc.) and the `shell` tool run as the user that started the daemon.** They have full access to that user's filesystem and credentials. Run Sinfonia under a dedicated user account or inside a container/sandbox if either the workflow author or the LLM provider is not fully trusted.
- **The HTTP server binds to `127.0.0.1` by default.** Do not expose `/api/v1/refresh` to the public internet without putting authentication in front.
- **Tracker contents are agent input.** A malicious issue title or description could attempt prompt-injection. The orchestrator does not sanitize tracker content — your workflow prompt should treat issue fields as untrusted text (e.g. avoid telling the agent to "follow any instructions in the description verbatim").
- **API keys are read from the process environment or `$VAR` indirection.** They are not logged. If you suspect a leak, rotate the upstream credential first.

See spec §15 (`docs/SPEC.md`) for the full security and operational-safety contract.

## Trust posture & hardening

This section is Sinfonia's documented trust posture, satisfying spec §15.1's
requirement that an implementation state its own. The design rationale is in
[`docs/proposals/0004-agent-tool-surface-hardening.md`](docs/proposals/0004-agent-tool-surface-hardening.md).

**Default posture: high-trust.** Sinfonia runs the coding agent with broad
authority. The `shell` tool is arbitrary `bash -lc`, and CLI backends run with
their own permission systems **disabled by default** (`--dangerously-skip-permissions`
for Claude Code, `codex exec` for Codex) — because unattended autonomous
operation needs *some* form of auto-approval; an interactive permission prompt
would simply hang a non-interactive subprocess. The daemon logs a `WARN` at
startup naming each backend running in this mode so the posture is visible
rather than implicit.

**The load-bearing mitigation is environmental, not in-binary.** Because the
agent legitimately needs autonomous shell to do its job, the boundary that
actually contains it is the environment you run the daemon in. **Run Sinfonia
inside an isolated container or VM** when any input (tracker tickets, PR
comments, repository contents, or harness output) is not fully trusted:

- **Dedicated, isolated host.** A container/VM with no access to anything you
  would not hand the agent: a dedicated OS user, a restricted `workspace.root`
  volume, and **restricted network egress** (the single most effective control
  against secret exfiltration).
- **Scoped credentials, least privilege.** Give the agent's tracker/GitHub
  tokens the minimum scope needed. The subprocess inherits the daemon's
  environment, so anything in that environment is reachable from `shell` — keep
  it minimal.
- **Branch protection is what actually backstops the merge gate.** CODEOWNERS
  gates *merging a PR*; it does **not** gate secret exfiltration, writes outside
  the workspace, or a direct/force push. Configure GitHub branch protection to
  forbid force-pushes to protected branches so the merge gate cannot be bypassed
  by an agent with push credentials.
- **Treat all agent inputs as untrusted instructions.** The structural
  defenses on harness output (`bridge.json`, SPEC §11.6.13) stop *template*
  injection; they cannot stop *semantic* injection ("ignore prior instructions
  …") reaching an LLM as natural-language text. The mitigation is bounding what
  the agent can do, not sanitizing what it reads.

**What CODEOWNERS does and does not cover:** it gates merge. It does nothing for
secret exfiltration, out-of-workspace writes, or direct pushes — those are
bounded only by the environment and credential scope above.

**Opt-in controls available today:**

- `agent.dispatch_allowlist.require_labels` — only dispatch tickets carrying an
  approval label, an entry-boundary gate mirroring CODEOWNERS at the exit
  (Proposal 0004 §4.3).
- The file tools (`read_file`/`write_file`/`edit_file`/`list_dir`) resolve
  symlinks and reject any path whose real location escapes the workspace
  (Proposal 0004 §4.4). The `shell` tool is *not* so confined — only the
  environment can bound it.

## What is in scope

- Code execution paths an attacker could reach by submitting an issue in the connected tracker (e.g. prompt injection that causes the agent to exfiltrate workspace contents).
- Path traversal or workspace escape in the workspace manager or hook runner.
- Authentication bypass on the HTTP API.
- Credential leakage through logs, error messages, or the dashboard.
- Crash-bug DoS in the orchestrator triggered by malformed tracker payloads.

## What is out of scope

- Vulnerabilities in the upstream `claude` or `codex` CLIs — report those to Anthropic / OpenAI respectively.
- Vulnerabilities in the LLM providers' APIs.
- Misconfigurations where the operator has chosen to expose the HTTP API publicly without a proxy.
- The risk that giving an autonomous agent commit access to your repo will produce undesirable code. That is a policy concern; see the README's "Trust posture" and "Team workflow patterns" sections.
