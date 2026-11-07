// A test group named like the service it tests: inside the group, a bare
// name is the group's own member, never the service's member of the same
// name (the helper's call was resolved to `Checkout.signIn(request:)`).
test Checkout {
    @Inject let checkout: Checkout

    func signIn(email: String = "a", password: String = "b") -> String {
        return "{email}{password}"
    }

    test "a helper of the group" {
        expect signIn(password: "z") == "az"
        //     ^^^^^^ resolves: member shop.Checkout.signIn(email:password:)
        //     ^^^^^^^^^^^^^^^^^^^^^ type: String
        expect checkout.signIn(request: "r") == "r"
    }
}
