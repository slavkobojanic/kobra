// An unknown name in an interpolation is K1201 only: no K3202 or K3210 follows.
component Summary {
    view {
        box { "x" }
            .class(.summary)
    }

    style {
        .summary { color: {accnt}; }
    }
}
