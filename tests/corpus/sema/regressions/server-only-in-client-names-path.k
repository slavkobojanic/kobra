// K4007 carries a note naming the path to the client (diagnostics.md): without it,
// nothing says why `Indirect`, which injects only a service, is server-only.
config Keys {
    @Env("API_KEY") let key: String
}

service Server {
    @Inject let keys: Keys
}

service Indirect {
    @Inject let server: Server
}

component Card {
    @Inject let indirect: Indirect

    view {
        text { "x" }
    }
}
