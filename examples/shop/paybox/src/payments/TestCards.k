/// The card numbers paybox knows, and how it treats every other number.
///
/// Paybox is a test provider, so a card's number decides what happens to a
/// charge, as in a real provider's test mode:
///
/// | Number | Outcome |
/// |---|---|
/// | `4242 4242 4242 4242` | `.approved` |
/// | `4000 0000 0000 0002` | `.declined(.cardDeclined)` |
/// | `4000 0000 0000 9995` | `.declined(.insufficientFunds)` |
/// | `4000 0000 0000 0069` | `.declined(.expiredCard)` |
/// | another number of 12 to 19 digits that passes the Luhn check | `.approved` |
/// | anything else | `.invalid` |
///
/// Spaces and dashes between digits are ignored, as on a printed card.
/// This type has no dependencies, so every rule is unit tested directly.
public struct TestCards {
    /// The card that always succeeds.
    static let success: String = "4242424242424242"

    /// The card the issuer always declines.
    static let declined: String = "4000000000000002"

    /// The card whose account can never cover the amount.
    static let insufficientFunds: String = "4000000000009995"

    /// The card that has always expired.
    static let expired: String = "4000000000000069"

    /// What a charge to `number` does.
    ///
    /// - Parameter number: the card number as the customer typed it.
    /// - Returns: the outcome; never fails.
    static func outcome(number: String) -> CardOutcome {
        guard let digits = TestCards.digits(of: number) else {
            return CardOutcome.invalid
        }

        let text = digits.map { digit => "{digit}" }.joined(separator: "")

        if text == TestCards.declined {
            return CardOutcome.declined(reason: .cardDeclined)
        }

        if text == TestCards.insufficientFunds {
            return CardOutcome.declined(reason: .insufficientFunds)
        }

        if text == TestCards.expired {
            return CardOutcome.declined(reason: .expiredCard)
        }

        return CardOutcome.approved
    }

    /// The last four digits of `number`, for receipts.
    ///
    /// - Parameter number: a card number `outcome(number:)` doesn't consider
    ///   invalid.
    /// - Returns: the last four digits, or `""` for an invalid number.
    static func lastFour(of number: String) -> String {
        guard let digits = TestCards.digits(of: number) else {
            return ""
        }

        var index = 0
        var last: List<String> = []

        for digit in digits {
            if index >= digits.count - 4 {
                last.append("{digit}")
            }

            index += 1
        }

        return last.joined(separator: "")
    }

    /// The digits of `number`, or `nil` unless it has 12 to 19 digits
    /// (after removing spaces and dashes), only digits, and passes the Luhn
    /// check.
    static func digits(of number: String) -> List<Int>? {
        let compact = number.replace(" ", with: "").replace("-", with: "")

        guard compact.count >= 12 and compact.count <= 19 else {
            return nil
        }

        var digits: List<Int> = []

        for character in compact.split(separator: "") {
            guard let digit = Int.parse(character) else {
                return nil
            }

            digits.append(digit)
        }

        guard TestCards.passesLuhn(digits) else {
            return nil
        }

        return digits
    }

    /// Whether `digits` pass the Luhn check: from the right, every second
    /// digit is doubled (less 9 when that is more than 9), and the sum of
    /// all of them must be a multiple of 10.
    static func passesLuhn(_ digits: List<Int>) -> Bool {
        var sum = 0
        var index = 0

        for digit in digits {
            let fromRight = digits.count - 1 - index

            if fromRight % 2 == 1 {
                let doubled = digit * 2
                sum += if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                sum += digit
            }

            index += 1
        }

        return sum % 10 == 0
    }
}
