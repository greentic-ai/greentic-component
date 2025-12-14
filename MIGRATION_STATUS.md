# Migration Status — Public-Launch Secrets

- What changed: component manifests now accept optional `secret_requirements` validated with the canonical `greentic-types` helpers (SecretKey pattern, scope env+tenant, schema-object). Host secrets capabilities expect structured requirements, templates/fixtures/runtime validation were updated accordingly, and the runtime no longer ships a secrets-store host binding; secret resolution is delegated to runner/greentic-secrets.
- What broke: manifests using string-only secrets or missing scope/format/schema-object now fail validation; pack tooling must read `secret_requirements` from the manifest JSON (not logs/sidecars).
- Next repos to update: pack/packc builders to ingest `secret_requirements` from component manifests; downstream consumers (dev/deployer/runner) should consume structured `SecretRequirement` entries instead of string lists and provide the actual secrets-store host binding.
