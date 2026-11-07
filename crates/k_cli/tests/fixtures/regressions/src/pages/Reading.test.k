test Reading {
    // A chained page parameter given as an implicit member is the enum
    // case of the parameter's type (it was lowered as a theme token).
    test "a chained enum parameter is the parameter's case" {
        let screen = render(Reading(id: 7).units(.imperial))

        expect screen.text("reading 7 in imperial").isVisible
    }

    test "an omitted query parameter takes its default" {
        let screen = render(Reading(id: 7))

        expect screen.text("reading 7 in metric").isVisible
    }

    // The same chained parameter on a `Page` value in test code.
    test "a chained enum parameter of a Page value is the parameter's case" {
        let reading: Page = .reading(id: 7)
        let page = reading.units(.imperial)

        expect "{page.url}" == "/reading/7?units=imperial"
        expect page != Page.reading(id: 7)
        expect "{reading.units(.imperial).url}" == "/reading/7?units=imperial"
    }
}
