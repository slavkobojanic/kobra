import k.rpc.RpcOnlyError

/// The page's way to the streams.
public service Live {
    @Inject let api: LiveProtocol

    @State var last: Int = 0

    func follow() async throws RpcOnlyError {
        let ticks = try await api.ticks()
        for await tick in ticks {
            last = tick.count
        }
        let counts = try await api.counts()
        for await count in counts {
            last = count
        }
    }

    func lost(because failure: RpcOnlyError) {
        last = -1
    }
}
