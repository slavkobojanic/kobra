// Links in a member's doc comment resolve against that member's parameters; an unknown one is K1901.
struct Cart {
    let size: Int

    /// Adds [`count`] lines; fails past [`maximumLines`].
    func add(count: Int) -> Int {
        return size + count
    }
}
