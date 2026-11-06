// A public extension: visible wherever `Int` is, without an import.
public extend Int {
    var cents: Int { self * 100 }
}
