/// A storage failure the user should hear about. None of them stops the
/// app: the boards keep working in memory.
public enum StorageProblem implements Equatable {
    /// Saved boards exist but could not be read, so the app started empty.
    /// The next successful save replaces them.
    case unreadable

    /// The browser refuses to store anything. Changes last until the tab
    /// is closed.
    case unavailable

    /// Storage is full, so the latest change was not saved.
    case full

    /// What to tell the user.
    var message: String {
        return match self {
            case .unreadable { "Your saved boards couldn't be read, so Kanban started empty." }
            case .unavailable { "This browser isn't saving anything; your boards last until you close the tab." }
            case .full { "Storage is full, so the latest change wasn't saved." }
        }
    }
}
