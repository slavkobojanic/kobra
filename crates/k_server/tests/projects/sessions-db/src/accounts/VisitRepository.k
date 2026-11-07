/// Visits.
public repository VisitRepository for Visit {
    func total() async -> Int {
        return await visits.count()
    }
}
