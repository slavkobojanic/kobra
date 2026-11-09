import k.storage.LocalStorage
import k.storage.StorageKey

/// The places the user opened most recently, newest first, kept in the
/// browser's local storage so they survive a reload.
///
/// Injecting `LocalStorage` makes this service client-only: the server can't
/// know what a browser has stored. That is why the start page shows the list
/// in a client-rendered component (`RecentPlacesList`) while the rest of the
/// page is rendered on the server.
public service RecentPlaces {
    @Inject let storage: LocalStorage

    /// How many places the list keeps.
    let limit: Int = 4

    // The key's name and the shape of `RecentPlace` are recorded in
    // storage.lock, so a change that would make saved lists unreadable fails
    // `k check`.
    private let savedPlaces: StorageKey<List<RecentPlace>> = StorageKey(name: "recentPlaces")

    /// The places, newest first.
    @State var places: List<RecentPlace> = []

    /// Restores the saved list. In the browser this runs after hydration, so
    /// the first view of the list already shows it.
    init() {
        do {
            places = try storage.read(key: savedPlaces) ?? []
        } catch .unreadable, .unavailable {
            places = []
        }
    }

    /// Moves `place` to the front of the list, drops the oldest beyond
    /// `limit`, and saves. Storage failures leave the list in memory only.
    func remember(place: Place) {
        let entry = RecentPlace(id: place.id, name: place.displayName)
        var next: List<RecentPlace> = [entry]
        for other in places {
            if other.id != entry.id and next.count < limit {
                next.append(other)
            }
        }
        places = next
        do {
            try storage.write(key: savedPlaces, value: places)
        } catch .quotaExceeded, .unavailable {
            // Remembering is a convenience; the page works without it.
        }
    }
}
