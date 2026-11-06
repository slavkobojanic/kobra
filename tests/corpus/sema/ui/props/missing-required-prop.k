// Omitting a required prop is K2003 at the insertion point before `)`; a call
// with the argument (the near miss) is fine.
component Greeting {
    @Prop let name: String

    view {
        text { name }
    }
}

component Home {
    view {
        Greeting()
        Greeting(name: "Ada")
    }
}
