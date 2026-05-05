# Event Tracking Overview — CodeScene MCP Server

## How It Works

Every time a user interacts with the MCP Server through their AI coding assistant, an anonymous usage event is recorded. These events help CodeScene understand which features are used and how often — without collecting any sensitive information.

```mermaid
flowchart TD
    subgraph USER ["👤 User's AI Coding Assistant"]
        A[User asks AI to review code quality]
        B[User asks AI to find tech debt]
        C[User asks AI to refactor code]
        D[User asks AI about code ownership]
        E[User asks AI for config or explanations]
    end

    subgraph MCP ["⚙️ CodeScene MCP Server"]
        F[Feature is executed]
        G[Anonymous event is created]
        H{Tracking enabled?}
    end

    subgraph PRIVACY ["🔒 Privacy Layer"]
        I["File & project names are replaced with anonymous hashes (originals are never sent)"]
    end

    subgraph DELIVERY ["📡 Event Delivery"]
        J["Sent in the background (never slows down the user)"]
        K[Silently skipped if network is unavailable]
    end

    subgraph DEST ["☁️ CodeScene Analytics"]
        L[Aggregated usage insights]
    end

    A & B & C & D & E --> F
    F --> G
    G --> H
    H -- Yes --> I
    H -- "No (user opted out)" --> X[No data sent]
    I --> J
    J --> DEST
    J --> K

    style USER fill:#e8f4fd,stroke:#2196F3,color:#000
    style MCP fill:#fff3e0,stroke:#FF9800,color:#000
    style PRIVACY fill:#e8f5e9,stroke:#4CAF50,color:#000
    style DELIVERY fill:#f3e5f5,stroke:#9C27B0,color:#000
    style DEST fill:#fce4ec,stroke:#E91E63,color:#000
    style X fill:#eeeeee,stroke:#9E9E9E,color:#000
```

## What Features Generate Events

| Category | Features | What's Recorded |
|---|---|---|
| **Code Quality** | Score a file, Review a file, Pre-commit check, Branch analysis | Quality score, number of files analyzed, pass/fail result |
| **Refactoring** | Auto-refactor, Business case analysis | Confidence level, target improvement score |
| **Tech Debt** | Hotspot discovery, Goal tracking | _(feature was used — no details)_ |
| **Collaboration** | Code ownership lookup | _(feature was used — no details)_ |
| **Configuration** | Read/write settings | Which setting was accessed |
| **Education** | Explain Code Health, Explain productivity impact | _(feature was used — no details)_ |
| **Errors** | Any feature that fails | Which feature failed and a generic error description |

## What Is Collected

```mermaid
flowchart LR
    subgraph COLLECTED ["✅ Collected"]
        direction TB
        C1[Which feature was used]
        C2[Anonymous server instance ID]
        C3[Deployment type — cloud or self-hosted]
        C4[Server version number]
        C5[Aggregated quality scores]
        C6[Pass / fail outcomes]
        C7[User identity via access token]
    end

    subgraph NOT_COLLECTED ["❌ Never Collected"]
        direction TB
        N1[File names or paths]
        N2[Source code contents]
        N3[Project or repository names]
        N4[IP address]
        N5[Company or organization info]
        N6[Conversation content with AI]
    end

    style COLLECTED fill:#e8f5e9,stroke:#4CAF50,color:#000
    style NOT_COLLECTED fill:#ffebee,stroke:#f44336,color:#000
```

## Privacy by Design

- **Anonymous hashing** — Any reference to a file or project is converted into an irreversible anonymous hash before it leaves the user's machine. The original names can never be recovered.
- **Non-blocking** — Events are sent in the background. Users never experience any delay.
- **Opt-out available** — Users can disable all tracking with a single configuration toggle. When disabled, zero data is sent.
- **No retry or storage** — If the network is unavailable, events are simply dropped. Nothing is queued or stored locally.
- **No personal data** — No IP addresses, emails, or any personally identifiable information is ever collected. User identity is associated only through the access token used for authentication.

## Deployment Flexibility

Events are sent to the appropriate endpoint based on how the customer is deployed:

| Deployment | Analytics Destination |
|---|---|
| **CodeScene Cloud** | CodeScene's hosted analytics endpoint |
| **Self-hosted / On-premises** | Customer's own CodeScene server |
| **Custom** | Configurable analytics URL |
