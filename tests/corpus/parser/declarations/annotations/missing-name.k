// `@` without a name reports K0100 at the next token; the declaration is still parsed.
struct A {
    @ let x: Int
    let y: Int
}
