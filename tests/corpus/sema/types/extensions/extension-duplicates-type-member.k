// A member an extension adds must not clash with a member the type already declares: K1305 at the second.
struct Point {
    let x: Int

    func norm() -> Int {
        return x
    }
}

extend Point {
    func norm() -> Int {
        return 0
    }
}
