/// A truth value, `true` or `false`.
///
/// The logical operators are the words `and`, `or` and `not`. Comparing a
/// `Bool` with a literal (`x == true`) is a lint.
@Native
public struct Bool implements Equatable, Hashable, Comparable, Codable, ConfigValue {}
