// An extension is not the declaration's body: it can't read the declaration's private members, even in the same file.
struct Account {
    private let pin: Int
}

extend Account {
    func hasPin() -> Bool {
        return pin > 0
    }
}
