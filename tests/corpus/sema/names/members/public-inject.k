// `public` on an `@Inject` field is K1303; the fix removes it.
import k.ui.Theme

component ThemeName {
    @Inject public let theme: Theme

    view {
        paragraph { "Theme" }
    }
}
