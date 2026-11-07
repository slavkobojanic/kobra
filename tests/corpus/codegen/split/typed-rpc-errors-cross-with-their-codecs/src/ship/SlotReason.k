/// Why there is no delivery slot.
public enum SlotReason implements Codable {
    case tooLate
    case tooEarly
}
