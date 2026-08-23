# Security Policy

## Reporting a vulnerability

Report suspected vulnerabilities privately via
[GitHub Private Vulnerability Reporting](https://github.com/ZirekHQ/pact-avro-plugin/security/advisories/new)
(Security tab → Report a vulnerability). Do not open a public issue for a suspected
vulnerability.

Include, where possible: the affected module/version, a minimal reproduction, and the impact
(e.g. malformed input handling, resource exhaustion, information disclosure).

## Scope

This plugin implements the [Pact plugin protocol](https://github.com/pact-foundation/pact-plugins)
to serialize/deserialize Pact contract test messages using [Apache Avro](https://avro.apache.org/docs).
The `plugin` module (its gRPC-based plugin interface and Avro schema handling) is in scope.
Denial-of-service reports against malformed Avro schemas or messages are in scope;
resource-exhaustion reports against large-but-well-formed inputs are lower priority.

## Supported versions

This project does not yet maintain parallel release branches — security fixes land on `main`.
