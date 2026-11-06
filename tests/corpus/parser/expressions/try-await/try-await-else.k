// `try await x else { ... }`: the else belongs to the outer `try`
func f() {
    let user = try await api.me() else {
        log("anonymous")
        return
    }
}
