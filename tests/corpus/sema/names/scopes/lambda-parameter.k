// A lambda parameter is a local of the lambda's body.
func doubled(values: List<Int>) -> List<Int> {
    return values.map { value => value * 2 }
    //                           ^^^^^ resolves: local value 3:25
    //            ^^^ resolves: member List.map(_:)
}
