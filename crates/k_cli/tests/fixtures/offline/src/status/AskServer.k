/// "Ask the server": calls the rpc from the click and shows the answer, or
/// why there is none.
public component AskServer {
    @Inject let greeting: Greeting

    @State var said: String = ""

    view {
        button { "Ask the server" }
            .on(click: ask)

        paragraph { said }
            .role(.status)
    }

    func ask() async {
        said = await greeting.ask()
    }
}
