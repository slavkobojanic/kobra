public page Home at "/" {
    @Inject let memo: MemoRecorder

    view {
        button { "Record" }
            .on(click: { await memo.start() })
        if let stream = memo.stream {
            audio(source: .stream(stream))
                .label("Live")
        }
    }
}
