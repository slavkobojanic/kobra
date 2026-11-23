// At most one unnamed preview, and no two of the same name.
component Badge {
    view {
        "badge"
    }
}

preview {
    Badge()
}

preview {
    Badge()
}

preview "empty" {
    Badge()
}

preview "empty" {
    Badge()
}
