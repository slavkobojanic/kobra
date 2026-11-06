// Found by the property `well_typed_projects_have_no_errors`
// (crates/k_sema/tests/properties.rs): the expected type `Int?` reaches the
// operand of a unary `-`, so the `if` expression is typed `Int?` and `-` is
// rejected (K2013). The operand is an `Int`, wrapped into `Int?` only after
// negation, as `1 + (if c { 1 } else { 2 })` and `-n` already are.
func negate(c: Bool, n: Int) {
    let a: Int? = -(if c { 1 } else { 2 })
    let b: Double? = -(if c { 1.5 } else { 2.5 })
    let fine: Int? = 1 + (if c { 1 } else { 2 })
    let alsoFine: Int? = -n
}
