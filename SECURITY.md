# Security Guidance

Greentic components often manipulate tenant secrets and network credentials. Follow these guardrails when authoring manifests or tooling:

- **Redaction (`x-redact`)** – Any JSON Schema field that may contain credentials must be annotated with `"x-redact": true`. Example:
  ```json
  {
    "type": "object",
    "properties": {
      "api_key": {
        "type": "string",
        "x-redact": true,
        "description": "Never logged or echoed back"
      }
    }
  }
  ```
  Runtimes use `schema::collect_redactions` to scrub transcript/log output automatically.
- **Capability declarations** – Only request the exact HTTP domains, secret scopes, KV buckets, filesystem paths, or tool invocations you need. `security::enforce_capabilities` will block mismatches, so keep manifests honest and minimal.
- **No secrets in logs** – Operators must treat `component-doctor`/`component-inspect` output as safe to share. Avoid printing raw config payloads and lean on `x-redact` plus the built-in capability summaries instead of dumping sensitive values.
- **Published schema** – Pin against the official schema served at <https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json>. The CI job curls this URL and ensures the `$id` matches the copy in-tree to catch any drift.

Report potential vulnerabilities privately by opening a Security Advisory on GitHub. Please include the component manifest, describe payload (with secrets removed), and the exact CLI output from `component-doctor`.
