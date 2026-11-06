// The conflict is by name: a computed property and a function named alike also clash.
extend Int {
    var half: Int {
        self / 2
    }
}

extend Int {
    func half() -> Int {
        return self / 2
    }
}
