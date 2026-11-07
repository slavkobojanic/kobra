/// The environments `use … in` rules choose between
/// (`docs/design/23-app-structure.md`, "Environments").
public enum Environment {
    /// The app run by `k dev`.
    case development
    /// The app run by `k test`.
    case test
    /// The app built by `k build`.
    case production
}
