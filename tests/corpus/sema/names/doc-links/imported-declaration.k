// A link may name a declaration the file imports.
import k.time.Clock

/// Stamps records with the injected [`Clock`]'s [`Clock.now`].
service Stamps {
    @Inject let clock: Clock
}
