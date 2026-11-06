/// Marks a standard-library type as a guard usable in `requires` clauses of
/// pages, protocols and rpcs (`docs/design/26-auth-and-sessions.md`,
/// "Guards"). A guard written without arguments names the type; with
/// arguments it calls one of its initializers.
@Annotation
public struct Guard {}
