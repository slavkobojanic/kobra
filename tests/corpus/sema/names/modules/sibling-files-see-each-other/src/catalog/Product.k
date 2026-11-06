// Declarations of one directory see each other across files, in both directions.
public struct Product {
    let name: String

    func badge() -> Badge {
        return Badge(text: name)
        //     ^^^^^ resolves: decl catalog.Badge
    }
}
