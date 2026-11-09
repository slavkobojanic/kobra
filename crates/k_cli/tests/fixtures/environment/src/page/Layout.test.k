import k.device.ViewportSize
import k.test.TestViewport

test Layout {
    @Inject let viewport: TestViewport

    test "a 1280 by 800 viewport with a mouse by default" {
        let screen = render(Layout())

        expect viewport.size == ViewportSize(width: 1280px, height: 800px)
        expect screen.text("Dialog").isVisible
        expect screen.text("Click to open").isVisible
        expect not screen.text("No hover").exists
    }

    test "a phone gets the sheet and taps" {
        let screen = render(Layout())

        viewport.resize(width: 390px, height: 844px)
        viewport.set(pointer: .coarse)
        viewport.set(canHover: false)

        expect screen.text("Phone sheet").isVisible
        expect screen.text("Tap to open").isVisible
        expect screen.text("No hover").isVisible
    }
}
