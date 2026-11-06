/// A struct whose constraint annotations and `validate(rules:)` function are
/// checked on the client by `@Form` and on the server before an rpc handler
/// runs (`docs/design/15-forms-and-validation.md`).
public interface Validatable {
    /// Cross-field rules. The default checks nothing.
    func validate(rules: Rules) {
    }
}
