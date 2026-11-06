// `key` is contextual: in `for key in keys key key`, only the word after the iterable is the keyword.
component T {
    view {
        for key in keys key key {
            text { key }
        }
    }
}
