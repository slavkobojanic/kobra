/// Collects cross-field failures in `validate(rules:)`.
@Native
public class Rules {
    /// Records `message` under `field` unless `condition` holds. `field` is
    /// a case of the struct's generated `FieldName` enum:
    /// `rules.check(confirm == password, field: .confirmPassword, message: "…")`.
    func check(_ condition: Bool, field: FieldName, message: String)
}
