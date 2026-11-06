// Nested lambdas without parameter types and without an expected type: one
// K2002 for the outermost, not one per level (the inner ones have nothing to
// infer from because the outer one has nothing).
func f() {
    let a = { x => { y => { z => 1 } } }
}
