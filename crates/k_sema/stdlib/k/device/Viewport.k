/// The layout viewport and the primary pointer
/// (`docs/design/45-web-platform.md`, "Viewport"), for decisions CSS can't
/// make; layout by width belongs in style blocks' media and container
/// queries. A stdlib service in `.app` scope, usable on both sides: inject
/// it with `@Inject let viewport: Viewport`, and [`k.test.TestViewport`]
/// replaces it in tests.
///
/// Every member is `nil` during server rendering, which doesn't know the
/// screen, and is reactive in the browser. Changes are coalesced to one
/// per animation frame.
@Native
public service Viewport {
    /// The layout viewport (`innerWidth`, `innerHeight`).
    var size: ViewportSize?
    /// The primary pointer (`pointer: fine`, `coarse` or `none`).
    var pointer: PointerKind?
    /// Whether the primary pointer can hover (`hover: hover`).
    var canHover: Bool?
}
