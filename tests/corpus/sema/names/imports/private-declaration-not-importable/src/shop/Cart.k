// Importing it is K1102 at the last segment, like a package-private one; the use reports nothing more.
import catalog.Hidden

func identify(hidden: Hidden) -> Int {
    return hidden.id
}
