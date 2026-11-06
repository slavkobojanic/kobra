// `@Layout` names a layout; `@NoLayout` opts a page out of the app's default.
layout Shop {
    @Slot let content: View

    view {
        content
    }
}

@Layout(Shop)
//      ^^^^ resolves: decl corpus.Shop
page Home {
    view {
        text { "home" }
    }
}

@NoLayout
page Embed {
    view {
        text { "embed" }
    }
}
