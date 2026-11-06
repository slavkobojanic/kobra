// the `key` clause is accepted on every `for`
func f() {
    for item in items key item.id {
        CartRow(item: item)
    }
}
