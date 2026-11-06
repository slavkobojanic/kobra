// An optional may be compared with `nil`.
func isMissing(name: String?) -> Bool {
    return name == nil
         //^^^^^^^^^^^ type: Bool
}
