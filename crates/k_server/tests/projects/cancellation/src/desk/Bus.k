/// Keeps listeners that one page registers and any page fires: a function
/// value that outlives the page that made it.
public service Bus {
    var listeners: List<() async -> Void> = []
    var plain: List<() -> Void> = []

    func listen(_ f: () async -> Void) {
        listeners.append(f)
    }

    func listenPlain(_ f: () -> Void) {
        plain.append(f)
    }

    func fire() async {
        for f in listeners {
            await f()
        }
        for g in plain {
            g()
        }
    }
}
