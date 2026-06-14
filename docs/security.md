# Security And Privacy

## Data Classification

Treat all of the following as sensitive:

- screenshots;
- OCR text and bounding boxes;
- application and window names;
- browser URLs;
- embeddings;
- grounded prompts and generated answers;
- databases and logs;
- API keys and sidecar bearer tokens.

None of these belong in Git.

## Network Boundaries

Default local services:

| Service | Binding |
|---|---|
| Application API | `127.0.0.1:3131` |
| Quality sidecar | `127.0.0.1:3132` |
| Bundled generation server | `127.0.0.1:31130` |

Do not bind the application API to `0.0.0.0` without adding authentication and
a deliberate network threat model. Automation endpoints can control the
desktop.

The application can make outbound requests for:

- first-use Hugging Face model downloads;
- first-use Paddle model downloads;
- bundled generation-model and llama-server downloads;
- optional remote OpenAI-compatible generation.

OCR, embeddings, sqlite-vec search, and reranking remain local. When remote
generation is configured, selected OCR context and frame metadata leave the
machine.

## Sidecar Authentication

The main process creates an ephemeral bearer token unless
`SCREENSEARCH_AI_SIDECAR_TOKEN` is already set. It passes the token to the
sidecar and authenticates requests.

Do not log or persist the token. The sidecar remains loopback-only even with
authentication enabled.

## Capture Exclusions

Configure excluded applications in `config.toml` and the settings UI. Include
password managers, banking tools, authentication apps, and any private
communication applications that should never be captured.

Exclusions are preventive. They are preferable to deleting sensitive data
after capture.

## Storage

SQLite and screenshot files are not encrypted by default. Use full-disk
encryption for the user profile and restrict filesystem permissions.

Retention cleanup removes old database records and related vectors, but normal
filesystem deletion is not guaranteed secure erasure on SSDs.

## API Keys

Generation API keys are settings data. Never commit them to `config.toml`,
fixtures, screenshots, logs, or documentation examples.

Use local providers when screen context must not leave the machine.

## Query Safety

FTS5 query handling quotes user text before `MATCH` execution. SQL values use
bound parameters.

Remote generation URLs are validated before use. Keep URL validation strict to
reduce accidental access to unsafe endpoints.

## Model Supply Chain

Current sidecar requirements and model identifiers are explicit, but model
weights download at runtime. Production hardening should add:

- pinned model revisions;
- checksums or signed manifests;
- controlled download origins;
- cache ownership and permission checks;
- documented model update review.

These remain secondary hardening work beyond the immediate quality migration.

## Logs

Avoid logging:

- full OCR text;
- generation prompts;
- API keys;
- bearer tokens;
- remote-provider responses containing sensitive context.

Operational logs should contain model identifiers, readiness state, frame IDs,
latency, and sanitized error summaries.

## Reporting A Security Issue

Use a private security-reporting channel when available. Do not attach real
captures, databases, or tokens to public issues.
