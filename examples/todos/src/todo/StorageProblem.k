/// A storage failure the user should hear about.
///
/// None of these stop the app. The todo list keeps working in memory; only
/// keeping it across reloads is affected.
public enum StorageProblem implements Equatable {
    /// Saved todos exist but could not be read, for example because they were
    /// edited by hand, so the list started empty. The next successful save
    /// replaces the unreadable data.
    case unreadable

    /// The browser refuses to store anything, as some private-browsing modes
    /// do. Changes last only until the tab is closed.
    case unavailable

    /// Storage is full, so the latest change was not saved.
    case full
}
