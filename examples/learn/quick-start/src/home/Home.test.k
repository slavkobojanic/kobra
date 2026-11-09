import k.test.render

test Home {
    test "each person pays their part of the bill and the tip" {
        let screen = render(Home())

        screen.textField(labeled: "Bill").fill("50")
        screen.button(named: "20%").click()

        expect screen.text("Total with tip: $60.00").isVisible
        expect screen.text("Each person pays $30.00").isVisible
    }
}
