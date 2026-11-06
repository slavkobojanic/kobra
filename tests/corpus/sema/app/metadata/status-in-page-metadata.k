// `status(_:)` is allowed only inside `failed`, so the page-level block can't set it.
page Missing {
    metadata {
        title("Missing")
        status(.notFound)
    }

    view {
        text { "missing" }
    }
}
