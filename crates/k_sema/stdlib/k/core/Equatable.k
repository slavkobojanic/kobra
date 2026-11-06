/// A type whose values can be compared with `==` and `!=`.
///
/// Structs and enums derive it with `implements Equatable`: two values are
/// equal when every field (or the case and its payload) is equal. Types that
/// must not be compared, such as `Secret` and `Mac`, don't implement it, so
/// `==` on them is a compile error.
public interface Equatable {}
