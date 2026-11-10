test Home {
    test "adding a note puts the focus back in the field" {
        let screen = render(Home())

        screen.textField(labeled: "Note").fill("Buy milk")
        screen.button(named: "Add note").click()

        expect screen.item(containing: "Buy milk").exists
        expect screen.textField(labeled: "Note").isFocused
        expect screen.focused.text == ""
    }

    test "a failed submit focuses the first invalid field" {
        let screen = render(Home())

        screen.textArea(labeled: "Message").fill("Hello")
        screen.button(named: "Send").click()
        await screen.settle()

        expect screen.textField(labeled: "Name").isFocused
        expect screen.text("Sent 0").exists
    }

    test "jumping scrolls to the end and focuses its heading" {
        let screen = render(Home())

        expect not screen.focused.exists

        screen.button(named: "Jump to the end").click()

        expect screen.heading(named: "The end").wasScrolledIntoView
        expect screen.heading(named: "The end").isFocused
        expect not screen.heading(named: "Focusing").wasScrolledIntoView
    }
}
