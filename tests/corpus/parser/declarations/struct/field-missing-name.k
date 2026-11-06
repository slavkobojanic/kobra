// A field without a name reports K0100 at the `:`; the next member is unaffected.
struct A {
    let : Int
    let y: Int
}
