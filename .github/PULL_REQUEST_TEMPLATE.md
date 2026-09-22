### What this changes

<!-- A sentence or two. Link the issue it addresses, if there is one. -->

### How it was verified

<!--
Which tests cover it, and anything you checked by hand. If it changes
behaviour against a particular Pact implementation or Avro type, say which.
-->

### Checklist

- [ ] Tests cover the change (`cargo test --locked` in `modules/plugin-rs`; `sbt consumer/test provider/test` if the examples are affected)
- [ ] `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings` pass
- [ ] `sbt scalafmtCheckAll javafmtCheckAll` passes, if the examples changed
- [ ] Documentation updated, if the change is user-facing
- [ ] `ci.yml` regenerated with `sbt githubWorkflowGenerate` if `build.sbt` changed
