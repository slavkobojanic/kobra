// Without an import they aren't in scope in another module (K1201, K1202), and they can't be imported, so no import fix is offered.
func count(helper: Helper) -> Int {
    return helperCount() + helper.id
}
