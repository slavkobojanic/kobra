// A value of the child interface is usable where the parent is expected.
interface Identified {
    var id: Int
}

interface Named implements Identified {
    var name: String
}

func key(item: Identified) -> Int {
    return item.id
}

func keyOf(named: Named) -> Int {
    return key(item: named)
                   //^^^^^ type: corpus.Named
}
