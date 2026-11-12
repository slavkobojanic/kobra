/// Marks a public standard-library declaration that only the standard
/// library itself may use (`docs/design/48-releases-and-versioning.md`,
/// "Stability levels"): to user code it is not public, so importing it is
/// K1102 and naming it is K1201 or K1202, and the standard library
/// reference leaves it out.
@Annotation
public struct Internal {}
