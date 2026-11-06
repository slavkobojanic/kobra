// A non-public extension: its members are visible only within module `money`.
extend Int {
    var cents: Int { self * 100 }
}
