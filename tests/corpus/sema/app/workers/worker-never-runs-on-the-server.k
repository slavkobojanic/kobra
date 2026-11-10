// docs/design/55-workers.md, "Server rendering": a `@Worker` service is
// client-only. Calling it from code that can run while the server renders is
// K4012, while effects and handlers call it freely; server-only code that
// injects it is K4008.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
service Summaries {
    func summary(text: String) async throws WorkerOnlyError -> String {
        return text.truncated(length: 10, ending: "…")
    }
}

@Scope(.request)
service Audit {
    @Inject let summaries: Summaries
}

page Home at "/" {
    @Inject let summaries: Summaries

    @Load let first: String = try await summaries.summary(text: "A long first note")

    @State var shown: String = ""

    func summarize() async {
        do {
            shown = try await summaries.summary(text: "Clicked")
        } catch .workerFailure {
            shown = ""
        }
    }

    view {
        button { "Summarize" }
            .on(click: summarize)

        paragraph { shown }
    }

    loading {
        paragraph { "…" }
    }

    failed {
        paragraph { "Failed" }
    }
}
