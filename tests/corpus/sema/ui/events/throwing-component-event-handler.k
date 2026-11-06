// The rule holds for component events as for element events.
enum SaveError implements Error {
    case failed
}

component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit(1) })
    }
}

component Home {
    func record(value: Int) throws SaveError {
        throw .failed
    }

    view {
        Counter()
            .on(changed: record)
    }
}
