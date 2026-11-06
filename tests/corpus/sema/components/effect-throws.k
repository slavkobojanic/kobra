enum SyncError implements Error {
    case failed
}

component Sync {
    @Effect(on: [])
    func start() throws SyncError {
    }

    view {
        text { "syncing" }
    }
}
