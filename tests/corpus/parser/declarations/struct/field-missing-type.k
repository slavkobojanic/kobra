// `:` without a type is K0123 at the gap; the initializer still parses.
struct A {
    var x: = 1
}
