// `<` needs `Comparable`; structs don't derive it.
struct Version implements Equatable {
    let major: Int
}

func newer(a: Version, b: Version) -> Bool {
    return a.major > b.major or a > b
}
