// `while` conditions and `match` scrutinees also stop before `{`
func f() {
    while queue.hasMore { queue.pop() }
    match state.current {
        default { }
    }
}
