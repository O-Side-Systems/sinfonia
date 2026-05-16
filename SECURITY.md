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
