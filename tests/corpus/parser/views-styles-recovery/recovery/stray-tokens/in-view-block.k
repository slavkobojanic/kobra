// Stray closing delimiters on their own line in a view are skipped as K0122; the following node parses.
component T {
    view {
        box { "x" }
        ) ]
        text { "y" }
    }
}
