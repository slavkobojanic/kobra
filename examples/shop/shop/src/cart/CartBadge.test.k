import k.test.TestTabMessaging

test CartBadge {
    @Inject let tabs: TestTabMessaging
    @Inject let cart: CartService

    test "an empty cart says so, with nothing to empty" {
        let screen = render(CartBadge())

        expect screen.text("Cart: 0 items").isVisible
        expect not screen.button(named: "Empty cart").exists
    }

    test "adding tells the other tabs" {
        let screen = render(CartBadge())
        let mug = CartItem(slug: "k-mug", name: "K Mug")

        cart.add(mug)
        await screen.settle()

        expect screen.text("Cart: 1 item").isVisible
        expect tabs.posted(on: cartChanges) == [.added(item: mug)]
    }

    test "a product another tab added shows here, and emptying the cart tells them" {
        let screen = render(CartBadge())
        let mug = CartItem(slug: "k-mug", name: "K Mug")

        tabs.deliver(on: cartChanges, message: .added(item: mug))
        tabs.deliver(on: cartChanges, message: .added(item: mug))
        await screen.settle()

        expect screen.text("Cart: 2 items").isVisible
        expect tabs.posted(on: cartChanges).isEmpty

        screen.button(named: "Empty cart").click()

        expect screen.text("Cart: 0 items").isVisible
        expect tabs.posted(on: cartChanges) == [.cleared]
    }
}
