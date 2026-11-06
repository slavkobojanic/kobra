// Each `@Scope` value from docs/design/10-services.md.
@Scope(.app) service A {}
@Scope(.request) service B {}
@Scope(.route) service C {}
@Scope(.component) service D {}
