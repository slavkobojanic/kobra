import k.test.TestClipboard

test ListeningPanel {
    @Inject let clipboard: TestClipboard

    test "a copy fires copied" {
        let screen = render(ListeningPanel())

        screen.button(named: "Copy code").click()
        await screen.settle()

        expect clipboard.text == "secret code"
        expect screen.text("Copied 1 times").isVisible
    }

    test "a refused copy fires failed with the error, and the panel says it" {
        clipboard.failWrites(error: .denied)
        let screen = render(ListeningPanel())

        screen.button(named: "Copy code").click()
        await screen.settle()

        expect screen.text("Copying was refused").isVisible
        expect screen.text("Copied 0 times").isVisible
        expect screen.status().text == ""
    }

    test "without a clipboard the copy fails as unsupported" {
        clipboard.setSupported(false)
        let screen = render(ListeningPanel())

        screen.button(named: "Copy code").click()
        await screen.settle()

        expect screen.text("No clipboard here").isVisible
        expect clipboard.text == nil
    }
}
