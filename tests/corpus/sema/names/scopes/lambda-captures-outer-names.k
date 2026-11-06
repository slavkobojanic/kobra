// A lambda body sees the enclosing parameters and locals.
func scaled(values: List<Int>, by factor: Int) -> List<Int> {
    let offset = 1
    return values.map { value => value * factor + offset }
    //                                   ^^^^^^ resolves: param factor 2:35
    //                                            ^^^^^^ resolves: local offset 3:9
}
