// An annotation argument may be any expression, such as a closed range.
struct Config {
    @Range(1...65535) @Env("PORT") let port: Int
}
