// A tuple pattern in `for` binds each of its names.
func labels(names: List<String>) -> List<String> {
    var result: List<String> = []
    for (index, name) in names.enumerated() {
        result.append("{index}: {name}")
        //              ^^^^^ resolves: local index 4:10
        //                       ^^^^ resolves: local name 4:17
    }
    return result
}
