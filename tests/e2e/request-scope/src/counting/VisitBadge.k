/// A component that reads the request's `Visits`.
public component VisitBadge {
    @Inject let visits: Visits

    view {
        paragraph { "badge sees {visits.count}" }
    }
}
