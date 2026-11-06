/// The type of an annotation parameter that takes the name of a declaration
/// written as an expression, such as `Dashboard` in `@Layout(Dashboard)` or
/// `Clock` in `@TestDefault(replaces: Clock)`.
///
/// The argument must be a name that resolves to a declaration; the
/// annotation table says which kinds of declaration are accepted.
@Native
public struct DeclarationName {}
