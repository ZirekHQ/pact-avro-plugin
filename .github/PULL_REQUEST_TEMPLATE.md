### What this changes

<!-- A sentence or two. Link the issue it addresses, if there is one. -->

### How it was verified

<!--
Which tests cover it, and anything you checked by hand. If it changes
behaviour against a particular Pact implementation or Avro type, say which.
-->

### Checklist

- [ ] Tests cover the change (`cargo test --locked`; if plugin behavior changed, build and install the current binary via `scripts/pluginLocalInstall.sh`, then run `cargo test --test e2e_consumer --test e2e_provider -- --include-ignored`)
- [ ] `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings` pass
- [ ] Documentation updated, if the change is user-facing
