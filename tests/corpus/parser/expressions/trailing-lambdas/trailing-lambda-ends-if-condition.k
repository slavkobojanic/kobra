// in an `if` condition a `{` opens the block, so `items.isEmpty { ... }` is not a call
func f() {
    if items.isEmpty { empty() }
}
