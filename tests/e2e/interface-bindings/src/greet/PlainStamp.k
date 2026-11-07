/// The only `Stamp`.
public service PlainStamp implements Stamp {
    func stamp() -> String {
        return "(stamped)"
    }
}
