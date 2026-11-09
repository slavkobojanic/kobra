test Measured {
    test "resized passes the content box's size" {
        let screen = render(Measured())

        expect screen.text("Not measured").isVisible

        screen.text("Measured area").resize(width: 320px, height: 200px)

        expect screen.text("Three columns").isVisible

        screen.text("Measured area").resize(width: 100px, height: 50px)

        expect screen.text("One column").isVisible
        expect screen.text("Resized 2 times").isVisible
    }

    test "an async handler gets the size too" {
        let screen = render(Measured())

        screen.text("Async area").resize(width: 320px, height: 200px)
        await screen.settle()

        expect screen.text("Three columns").isVisible
    }
}
