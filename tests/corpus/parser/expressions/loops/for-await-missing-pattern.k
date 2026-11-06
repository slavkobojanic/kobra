// `for await` with nothing after it reports the missing pattern once.
func f() {
    for await in changes { }
}
