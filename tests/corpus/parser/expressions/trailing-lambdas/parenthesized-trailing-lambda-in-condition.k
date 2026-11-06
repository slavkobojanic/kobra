// parentheses lift the restriction: `if (items.contains { i => i.isNew }) { ... }`
func f() {
    if (items.contains { i => i.isNew }) { badge() }
}
