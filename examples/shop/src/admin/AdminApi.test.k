import k.rpc.RpcOnlyError
import k.test.TestClock
import k.text.EmailAddress
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository

// Integration tests: AdminService calls AdminApi in-process through
// AdminProtocol, so the rpcs' `requires` guards and request validation run
// exactly as they do over the network.

test AdminApi {
    use AdminConfig.sample for AdminConfig

    @Inject let admin: AdminService
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let clock: TestClock

    func signIn(email: EmailAddress = EmailAddress("admin@shop.test"), password: String = "change-me-admin-password") async throws SignInError {
        try await admin.signIn(request: SignInRequest(email: email, password: password))
    }

    // Signing in

    test "the configured email and password sign in" {
        try await signIn()

        expect try await admin.recentOrders().isEmpty
    }

    test "the email is matched without regard to case" {
        try await signIn(email: EmailAddress("Admin@Shop.TEST"))

        expect try await admin.recentOrders().isEmpty
    }

    test "a wrong password is refused" {
        expect throws SignInError.invalidCredentials {
            try await signIn(password: "change-me")
        }
    }

    test "an unknown email is refused in exactly the same way" {
        expect throws SignInError.invalidCredentials {
            try await signIn(email: EmailAddress("intruder@shop.test"))
        }
    }

    test "a blank password is refused by validation before the handler runs" {
        expect throws SignInError.rpcFailure(.invalid) {
            try await signIn(password: "")
        }
    }

    test "the sixth attempt within a minute is rate limited, even with the right password" {
        for _ in 0..<5 {
            expect throws SignInError.invalidCredentials {
                try await signIn(password: "guess")
            }
        }

        expect throws SignInError.rpcFailure(.rejected(.rateLimited)) {
            try await signIn()
        }

        clock.advance(by: 1.minute)

        try await signIn()
    }

    // Guards

    test "orders can't be listed without signing in" {
        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await admin.recentOrders()
        }
    }

    test "signing out ends the session" {
        try await signIn()
        try await admin.signOut()

        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await admin.recentOrders()
        }
    }

    // Orders

    test "recent orders are listed newest first" {
        let mug = Product.mug()
        let older = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(older)
        clock.advance(by: 1.minute)

        let newer = Order.sample(product: mug, placedAt: clock.now)
        await orders.insert(newer)
        try await signIn()

        let listed = try await admin.recentOrders()

        expect listed.map { order => order.id } == [newer.id, older.id]
    }
}
