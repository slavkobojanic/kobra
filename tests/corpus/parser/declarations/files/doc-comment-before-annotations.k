// A doc comment comes before the annotations of the declaration it documents.
/// The app.
@Layout(Main)
public app Shop {
    /// Fake payments everywhere but production.
    use Fake for Payments in .development, .test
}
