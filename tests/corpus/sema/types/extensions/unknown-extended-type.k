// Extending an unknown type is K1202 only; its members are not checked against anything.
extend Pointe {
    func norm() -> Int {
        return 0
    }
}
