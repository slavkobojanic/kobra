import k.storage.LocalStorage
import k.storage.StorageKey

/// Remembers which color scheme the user chose.
///
/// The choice is saved to local storage, so it survives reloads. Losing it is
/// harmless, so storage failures are ignored rather than reported, and the
/// app falls back to the light scheme.
///
/// The service only records the choice. Applying it to the document is left
/// to the page that renders the app, which keeps view concerns out of this
/// service and keeps it easy to test.
public service AppearanceService {
    @Inject let storage: LocalStorage

    private let savedScheme: StorageKey<ColorScheme> = StorageKey(name: "colorScheme")

    /// The color scheme in use. `.light` until the user chooses.
    @State var scheme: ColorScheme = .light

    /// Restores the saved choice as soon as the service is created, so the
    /// first render already uses it.
    init() {
        restore()
    }

    /// Switches to `scheme` and remembers the choice.
    ///
    /// - Parameter scheme: the scheme to use from now on. It is used even
    ///   when it can't be saved.
    func choose(scheme: ColorScheme) {
        self.scheme = scheme

        try storage.write(key: savedScheme, value: scheme) else { return }
    }

    /// Replaces the current scheme with the saved one. Keeps the current
    /// scheme when nothing is saved, or when the saved value can't be read.
    func restore() {
        let saved = try storage.read(key: savedScheme) else { return }

        if let saved {
            scheme = saved
        }
    }
}
