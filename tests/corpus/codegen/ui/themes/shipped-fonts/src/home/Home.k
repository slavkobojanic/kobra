public page Home {
    metadata { title("Home") }

    view {
        heading(level: 1) { "Ana & Luka" }
            .class(.title)
    }

    style {
        .title { font: .heading; }
    }
}
