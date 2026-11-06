// path: src/shop/Samples.k
// A file holding only extensions has no public declaration, so it is named after its purpose.
public extend Int {
    var dozen: Int {
        self * 12
    }
}

extend String {
    var shout: String {
        uppercased()
    }
}
