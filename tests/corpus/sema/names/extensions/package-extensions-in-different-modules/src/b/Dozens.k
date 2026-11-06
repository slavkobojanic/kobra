// The same member in module `b`: never both visible in one place, so no K1305.
extend Int {
    var dozen: Int {
        self * 13
    }
}
