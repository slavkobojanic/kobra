import k.test.TestConnectivity

test AskServer {
    @Inject let connectivity: TestConnectivity

    test "online, the server answers" {
        let screen = render(AskServer())

        screen.button(named: "Ask the server").click()
        await screen.settle()

        expect screen.text("Hello from the server").isVisible
    }

    test "offline, the rpc fails with rpcFailure(.offline)" {
        let screen = render(AskServer())

        connectivity.goOffline()
        screen.button(named: "Ask the server").click()
        await screen.settle()

        expect screen.text("The server can't be reached: you're offline.").isVisible
    }
}
