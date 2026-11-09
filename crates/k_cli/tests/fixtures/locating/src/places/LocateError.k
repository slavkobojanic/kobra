/// Why [`LocationService.locate()`] found no position.
public enum LocateError implements Error, Equatable {
    case unsupported
    case denied
    case noFix
}
