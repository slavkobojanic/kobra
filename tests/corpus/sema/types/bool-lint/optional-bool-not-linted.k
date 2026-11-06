// Comparing a `Bool?` with a literal is meaningful (`nil` differs from `false`): no K2012.
func isOn(flag: Bool?) -> Bool {
    return flag == true
         //^^^^^^^^^^^^ type: Bool
}
