/// A client-allowed service that nothing injects.
public service Unused {
    func hello() -> String {
        return "unused-service-literal"
    }
}
