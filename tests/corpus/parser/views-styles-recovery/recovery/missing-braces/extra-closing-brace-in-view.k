// An extra `}` closes the view and then the component early; the remaining `loading {}` and `}` at the top level are skipped as one K0125 run.
component T {
    view {
        box { "x" } }
}
loading {}
}
