# Privacy

Codex Meter is a local desktop utility. It has no analytics, advertising SDK, telemetry endpoint, or independent upload service.

## Data accessed locally

- Codex account rate limits and usage returned by the local `codex app-server`
- Thread summaries needed to identify active tasks
- The tail of local Codex rollout event files, limited to 4 MB per inspected file
- Local application settings such as display mode, window position, refresh interval, renewal date, theme, opacity, and an optional custom Codex executable path

Rollout data is parsed in memory to extract timestamps, task lifecycle events, and Token counts. The application may display a local thread title as the current task title. This title can contain user-authored text.

## Data not collected or transmitted by Codex Meter

- Authentication tokens or API keys
- Account email or payment information
- Conversation message bodies as stored application data
- Usage analytics or crash telemetry

Codex Meter does not persist rollout contents or send them to the Codex Meter repository, its author, or another third-party service. The `codex app-server` process remains an OpenAI/ChatGPT component and follows the behavior of the installed Codex product.

## Storage

Settings are stored in the operating system's per-user application configuration directory. They can be removed by deleting the Codex Meter application data directory after quitting the application.

## Source audit

The public repository contains application source, generated icons, dependency lockfiles, and documentation. It does not contain a user's Codex logs, settings, credentials, or conversation content.
