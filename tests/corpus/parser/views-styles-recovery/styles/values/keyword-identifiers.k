// Bare identifiers are CSS keywords, written in camel case (`spaceBetween`); style mode has no K keywords.
component T {
    style {
        .a {
            borderStyle: solid;
            overflow: hidden;
            justifyContent: spaceBetween;
            fontFamily: "Inter", sansSerif;
            content: not;
        }
    }
}
