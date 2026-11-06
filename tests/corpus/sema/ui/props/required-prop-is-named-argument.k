// A prop without a default is required and passed as a named argument. The call
// is a `View`; the prop reads resolve to the component's members.
component Greeting {
    @Prop let name: String

    view {
        text { "Hello, {name}" }
        //              ^^^^ resolves: member corpus.Greeting.name
    }
}

component Home {
    view {
        Greeting(name: "Ada")
      //^^^^^^^^ resolves: decl corpus.Greeting
      //^^^^^^^^^^^^^^^^^^^^^ type: View
    }
}
