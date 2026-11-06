// A protocol header may declare guards for all its rpcs.
protocol AdminProtocol requires Authenticated, Role(.admin) {
    rpc stats() -> Stats
}
