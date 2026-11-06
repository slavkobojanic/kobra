// Another public extension adding the same member: both are visible everywhere, so this second one is K1305.
public extend Int {
    var dozen: Int {
        self * 13
    }
}
