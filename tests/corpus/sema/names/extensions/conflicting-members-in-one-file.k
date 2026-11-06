// Two visible extensions adding one member name to one type: K1305 at the second.
extend Int {
    var dozen: Int {
        self * 12
    }
}

extend Int {
    var dozen: Int {
        self * 13
    }
}
