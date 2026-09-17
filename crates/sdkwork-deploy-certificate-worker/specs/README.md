# Certificate Worker Component

`component.spec.json` declares the bounded issuance loop, its lease and
operation-timeout invariants, the service-host dependency that supplies the ACME
engine, and every environment key this worker or that engine reads.

The engine itself is not a crate of its own: it is `certificate_issuance` in
`sdkwork-intelligence-deploy-service` over the shared
`sdkwork-webserver-acme-service`. PLAN-2026-0003 §9 asks for "one ACME implementation
across the fleet", and depending on the crate the Web Server standalone manager
already uses is the strongest form of that — a second implementation kept in step by
discipline would drift the first time one side fixed a validation bug.
