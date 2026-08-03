### What this changes

<!-- A sentence or two. Link the issue it addresses, if there is one. -->

### How it was verified

<!--
Which tests cover it, and anything you checked by hand. If it changes
behaviour against a particular Pact implementation or Avro type, say which.
-->

### Checklist

- [ ] Tests cover the change (`plugin/test`, and `consumer/test` if the DSL is affected)
- [ ] `sbt scalafmtCheckAll javafmtCheckAll` passes
- [ ] Documentation updated, if the change is user-facing
- [ ] Workflows regenerated with `sbt githubWorkflowGenerate` if the build changed
