test Overview {
    // `Page.overview` names the case with its defaults, so a chained
    // parameter sets one field of a whole page value (it spread the case's
    // constructor function, which has no fields, and lost the page).
    test "a page case with only optional parameters is a value" {
        expect "{Page.overview.url}" == "/overview"
        expect "{Page.overview.units(.imperial).url}" == "/overview?units=imperial"
    }
}
