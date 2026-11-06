// `static static` is K0127 on the second `static`.
struct A {
    static static let x = 1
}
