// the `for` iterable stops before `{`, so `for x in xs.all { ... }` iterates `xs.all`
func f() {
    for x in xs.all { use(x) }
}
