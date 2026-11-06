// Pages and layouts declare typed `metadata` entries; `canonical` takes a `Page`.
layout Shop {
    @Slot let content: View

    metadata {
        title("Acme Shop")
    }

    view {
        content
    }
}

@Layout(Shop)
page Home {
    metadata {
        title("Home · Acme Shop")
        description("Everything for your desk.")
        canonical(.home)
        //        ^^^^^ type: Page
    }

    view {
        text { "home" }
    }
}
