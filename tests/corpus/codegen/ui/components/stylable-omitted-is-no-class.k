// `@Stylable` props are optional (06-styles.md): a parent that doesn't set
// one gives the child no extra class, so the child defaults it to none
// before reading it.
component Badge {
    @Stylable let container: Classes

    view {
        box { "new" }
            .class([.badge, container])
    }

    style {
        .badge { padding: 2px; }
    }
}

component Shelf {
    view {
        Badge()
    }
}
