// `private extend` is K1304; the fix removes `private`, making the extension module-wide.
private extend Int {
    var doubled: Int {
        self * 2
    }
}
