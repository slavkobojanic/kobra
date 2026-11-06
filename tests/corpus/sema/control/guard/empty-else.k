// An empty `else` block doesn't exit either.
func check(ready: Bool) {
    guard ready else {}
    start()
}

func start() {}
