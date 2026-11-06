// `Secret<T>` has no `==` (K2001), but interpolating it is allowed: `"{s}"`
// renders `Secret(•••)`, so logging a config can't print the value
// (25-config-and-secrets.md, the `Secret<T>` table; sema.md §9.7).
func leak(s: Secret<String>) -> String {
    return "{s}"
}

func compare(s: Secret<String>) -> Bool {
    return s == s
}
