// Members of an extension follow the extension's modifier; `public` on one is K1302.
public extend Int {
    public var dozen: Int {
        self * 12
    }
}
