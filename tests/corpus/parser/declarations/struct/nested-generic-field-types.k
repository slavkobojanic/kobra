// `>>` in nested generics is two closing tokens.
struct Index {
    var byTag: Map<String, List<Map<Int, Set<String>>>> = [:]
    var pairs: List<(Int, String)> = []
}
