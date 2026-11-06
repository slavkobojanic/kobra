import k.storage.LocalStorage
import k.storage.StorageKey
import k.test.MemoryStorage

test AppearanceService {
    use MemoryStorage for LocalStorage

    @Inject let storage: MemoryStorage
    @Inject let appearance: AppearanceService

    test "the light scheme is used until the user chooses" {
        expect appearance.scheme == .light
    }

    test "choosing a scheme uses it and saves it" {
        appearance.choose(scheme: .dark)
        let saved = try storage.read(key: savedSchemeKey())

        expect appearance.scheme == .dark
        expect saved == .dark
    }

    test "restoring picks up the saved scheme" {
        try storage.write(key: savedSchemeKey(), value: ColorScheme.dark)

        appearance.restore()

        expect appearance.scheme == .dark
    }

    test "an unreadable saved scheme keeps the current one" {
        storage.writeRaw(name: "colorScheme", text: "purple")

        appearance.restore()

        expect appearance.scheme == .light
    }

    test "blocked storage keeps the current scheme" {
        storage.failReads(error: .unavailable)

        appearance.restore()

        expect appearance.scheme == .light
    }

    test "a scheme that can't be saved is still used" {
        storage.failWrites(error: .quotaExceeded)

        appearance.choose(scheme: .dark)

        expect appearance.scheme == .dark
    }
}

/// The key `AppearanceService` saves under, spelled out because it is part of
/// the app's stored format.
func savedSchemeKey() -> StorageKey<ColorScheme> {
    return StorageKey<ColorScheme>(name: "colorScheme")
}
