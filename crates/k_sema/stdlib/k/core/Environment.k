/// The environments `use … in` rules choose between
/// (`docs/design/23-app-structure.md`, "Environments").
public enum Environment {
    /// `k dev`.
    case development
    /// `k test`.
    case test
    /// `k build`.
    case production
}
