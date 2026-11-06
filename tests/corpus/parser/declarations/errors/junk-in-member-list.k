// A token that cannot start a member is K0126; the next member still parses.
struct A {
    42
    let x: Int
}
