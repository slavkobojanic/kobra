/// One file of a font family a package ships, in a theme's
/// [`Font`]: `FontFace("fonts/Inter-Bold.woff2", weight: 700)`. It
/// becomes an `@font-face` rule that swaps the face in when it loads, and
/// every app that uses the theme serves the file under a content-hashed
/// name below `/assets/`; never from a third-party host. A face exists
/// only at compile time, so it has no members.
@Native
public struct FontFace implements Equatable, Codable {
    /// A face from a file of the package, checked when you compile: a
    /// string literal naming a `.woff2` or `.woff` file relative to the
    /// root of the package whose theme declares it.
    ///
    /// - Parameter file: the path, such as `"fonts/Inter-Regular.woff2"`.
    /// - Parameter weight: the CSS font weight the file draws, 1 to 1000.
    /// - Parameter style: whether the file draws upright or italic text.
    @Native
    @Literal
    init(_ file: String, weight: Int = 400, style: FontStyle = .normal) {}
}
