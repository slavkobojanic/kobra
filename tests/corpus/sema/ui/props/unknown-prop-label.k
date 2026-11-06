// A misspelled label is K2004 with the closest label as a fix. The missing
// `name` it was meant to be is not reported again as K2003.
component Greeting {
    @Prop let name: String

    view {
        text { name }
    }
}

component Home {
    view {
        Greeting(nme: "Ada")
    }
}
