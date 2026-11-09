/// The settings someone running Shelf can change without a new build.
public config ShelfConfig {
    /// Whether new people can make an account. Set it to `false` once
    /// everyone who should have a shelf has one.
    @Env("SHELF_SIGN_UPS") let signUpsOpen: Bool = true
}
