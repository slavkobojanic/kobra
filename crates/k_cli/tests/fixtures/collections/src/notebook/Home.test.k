import k.test.MemoryCollections

test Home {
    @Inject let collections: MemoryCollections

    test "writing an entry shows it and the count" {
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Entry").fill("hello")
        screen.button(named: "Write").click()
        await screen.settle()

        expect screen.text("1 entries").isVisible
        expect screen.text("hello").isVisible
    }

    test "the first build doesn't offer topics" {
        let screen = render(Home())
        await screen.settle()

        expect not screen.button(named: "Find tagged").exists
    }
}
