// Two imports may not bind one simple name: K1106 at the second, without a fix because they name different declarations. The use of `Item` reports nothing more.
import a.Item
import b.Item

func describe(item: Item) -> Int {
    return 0
}
