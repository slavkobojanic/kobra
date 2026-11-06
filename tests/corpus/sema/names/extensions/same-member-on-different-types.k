// The same member name on two different types is no conflict.
extend Int {
    var dozen: Int {
        self * 12
    }
}

extend Double {
    var dozen: Double {
        self * 12.0
    }
}
