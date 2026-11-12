// A file comment.
/// The point.
struct Point {
    // The x coordinate.
    let x: Int // trailing
    /* a block */ let y: Int
    /*
       A multi-line block comment,
         kept as written.
    */
    let z: Int
    // Before the closing brace.
}

func f() {
    let a = 1 /* inline */ + 2
    // A comment before else.
    if a > 1 {
        g()
    } // after the if block
    else {
        h()
    }
}

component Stepper {
    @Binding var value: Int   // required → named argument
    @Prop let step: Int = 1   // has a default → .step()
    @Prop let max: Int? = nil // optional → .max()

    @Prop let label: String // alone
    view { "x" }
}
