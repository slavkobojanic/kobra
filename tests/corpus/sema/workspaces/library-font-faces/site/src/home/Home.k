public page Home {
    metadata { title("Home") }

    view {
        paragraph { "Hello" }
    }

    func face() -> FontFace {
        return FontFace("fonts/Display.woff2")
    }
}
