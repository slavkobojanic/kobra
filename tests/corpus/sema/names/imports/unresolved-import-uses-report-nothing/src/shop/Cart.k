// An unresolved import is one K1101: the name it would bind reports nothing (no K1202), and the import isn't also unused (no K1105).
import catalog.Prodcut

func first(items: List<Prodcut>) -> Int {
    return items.count
}
