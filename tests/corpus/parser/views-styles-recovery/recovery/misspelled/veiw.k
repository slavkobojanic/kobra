// A misspelled `view` is not a member: K0126 at it, and the whole balanced block is skipped into one ErrorNode.
component T {
    veiw {
        box()
    }

    loading {}
}
