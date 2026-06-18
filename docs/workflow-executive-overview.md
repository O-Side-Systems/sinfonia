# How the System Works — Executive Overview

## 1. The Lifecycle — how a task moves from assigned to live

```mermaid
flowchart TD
    Start["👤 Person assigns a task"]:::human
    Queue["Task queue"]
    AI["🤖 AI does the work<br/>and proposes a change"]
    Checks["🧪 Automated quality checks<br/>grade the work"]
    Pass{"Passed all checks?"}
    Merge{"Still fits cleanly with<br/>the current codebase?"}
    Update["AI updates the change<br/>to match latest code"]
    Limit{"Too many<br/>fix attempts?"}
    Fixes["AI revises and tries again"]
    Review["👤 Person reviews<br/>and approves"]:::human
    Escalate["👤 Person steps in<br/>to unblock"]:::human
    Done["✅ Change goes live"]

    Start --> Queue --> AI --> Checks --> Pass
    Pass -->|"yes"| Merge
    Merge -->|"yes"| Review
    Merge -->|"no, out of date<br/>or conflicting"| Update --> Checks
    Pass -->|"no"| Limit
    Limit -->|"no, keep going"| Fixes --> AI
    Limit -->|"yes, stop"| Escalate
    Escalate --> Queue
    Review --> Done

    classDef human fill:#ffe8cc,stroke:#e8590c,stroke-width:2px;
```

**The key point:** the AI never ships on its own. A change must pass its quality
checks *and* still merge cleanly with the current codebase before it can reach a
person, and a person approves every change before it goes live.

---

## 2. The Guardrails — why it cannot run away

```mermaid
flowchart LR
    Work["🤖 Autonomous work<br/>(runs on its own,<br/>but within limits)"]
    G1{"Too many<br/>fix attempts?"}
    G2{"Spending<br/>too much?"}
    G3{"Task unclear or<br/>needs judgment?"}
    Continue["Keeps working<br/>within limits"]
    Human["👤 Lands on a<br/>person's desk"]:::human

    Work --> G1
    Work --> G2
    Work --> G3
    G1 -->|"no"| Continue
    G2 -->|"no"| Continue
    G3 -->|"no"| Continue
    G1 -->|"yes"| Human
    G2 -->|"yes"| Human
    G3 -->|"yes"| Human

    classDef human fill:#ffe8cc,stroke:#e8590c,stroke-width:2px;
```

**The key point:** three independent limits — number of attempts, cost, and
clarity — each route a stuck task to a person rather than letting it loop forever
or burn budget. Autonomy is real but bounded.
