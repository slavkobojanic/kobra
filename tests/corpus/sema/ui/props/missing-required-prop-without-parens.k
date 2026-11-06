// A component written without an argument list still needs its required props:
// K2003 sits right after the callee.
component Greeting {
    @Prop let name: String

    view {
        text { name }
    }
}

component Home {
    view {
        box {
            Greeting
        }
    }
}
