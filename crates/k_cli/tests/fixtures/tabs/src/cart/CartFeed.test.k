import k.test.TestTabMessaging

test CartFeed {
    @Inject let tabs: TestTabMessaging

    test "an effect follows the other tabs' messages" {
        let screen = render(CartFeed())
        await screen.settle()

        tabs.deliver(on: notices, message: "sale")
        tabs.deliver(on: notices, message: "restock")
        await screen.settle()

        expect screen.text("Heard 2").isVisible
    }
}
