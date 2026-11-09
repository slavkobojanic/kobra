/// Why "Use my location" found no position: `LocationService` folds the
/// five cases of `k.device.LocationError` into the three the start page
/// explains differently.
public enum LocateError implements Error, Equatable {
    /// The browser can't locate the device here.
    case unsupported

    /// The user refused, now or in the site's settings.
    case denied

    /// The device found no position, or not in time.
    case noFix
}
