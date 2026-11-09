import k.device.TabMessaging
import k.test.TestTabMessaging

test CartBadge {
    @Inject let tabs: TabMessaging
    @Inject let testTabs: TestTabMessaging

    test "adding posts the change to the other tabs" {
        let screen = render(CartBadge())

        screen.button(named: "Add a mug").click()
        screen.button(named: "Add a mug").click()
        screen.button(named: "Empty").click()

        expect screen.text("Cart 0").isVisible
        expect testTabs.posted(on: cartChanges) == [.added(name: "Mug", quantity: 1), .added(name: "Mug", quantity: 1), .cleared]
    }

    test "a change another tab posts updates this one" {
        let screen = render(CartBadge())

        testTabs.deliver(on: cartChanges, message: .added(name: "Notebook", quantity: 2))
        await screen.settle()

        expect screen.text("Cart 2").isVisible
        expect screen.text("Last Notebook").isVisible
        expect testTabs.posted(on: cartChanges).isEmpty
    }

    test "the injected interface is the double" {
        tabs.post(on: cartChanges, message: .cleared)

        expect testTabs.posted(on: cartChanges) == [.cleared]
    }
}
