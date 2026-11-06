// An implicit member resolves against the attribute's type: `.sumbit` is not a
// `ButtonType` case (K2006, closest case as a fix).
component Pay {
    view {
        button { "Pay" }
            .type(.sumbit)
    }
}
