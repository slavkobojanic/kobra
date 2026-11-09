/// A place the user opened, as the start page lists it: enough to link to its
/// dashboard and label the link.
public struct RecentPlace implements Codable, Equatable {
    /// The GeoNames id, as in [`Place`].
    let id: Int

    /// The place's display name, for example `"Berlin, Land Berlin, Germany"`.
    let name: String
}
