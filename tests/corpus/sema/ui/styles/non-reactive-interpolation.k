// `{ }` is only for reactive values (06-styles.md): a plain field is K3210 at the
// expression.
component Summary {
    let width: Length = 10px

    view {
        box { "x" }
            .class(.summary)
    }

    style {
        .summary { width: {width}; }
    }
}
