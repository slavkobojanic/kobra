test TestCards {
    // Outcomes

    test "the success card is approved" {
        expect TestCards.outcome(number: "4242424242424242") == .approved
    }

    test "the documented decline cards are declined for their reasons" {
        expect TestCards.outcome(number: "4000000000000002") == .declined(reason: .cardDeclined)
        expect TestCards.outcome(number: "4000000000009995") == .declined(reason: .insufficientFunds)
        expect TestCards.outcome(number: "4000000000000069") == .declined(reason: .expiredCard)
    }

    test "spaces and dashes are ignored, as on a printed card" {
        expect TestCards.outcome(number: "4242 4242 4242 4242") == .approved
        expect TestCards.outcome(number: "4000-0000-0000-0002") == .declined(reason: .cardDeclined)
    }

    test "another number that passes the Luhn check is approved" {
        expect TestCards.outcome(number: "5555555555554444") == .approved
        expect TestCards.outcome(number: "378282246310005") == .approved
    }

    test "a number that fails the Luhn check is invalid" {
        expect TestCards.outcome(number: "4242424242424241") == .invalid
    }

    test "letters, too few or too many digits are invalid" {
        expect TestCards.outcome(number: "4242abcd42424242") == .invalid
        expect TestCards.outcome(number: "42424242") == .invalid
        expect TestCards.outcome(number: "42424242424242424242") == .invalid
        expect TestCards.outcome(number: "") == .invalid
    }

    // Receipts

    test "the last four digits are the end of the number" {
        expect TestCards.lastFour(of: "4242 4242 4242 4242") == "4242"
        expect TestCards.lastFour(of: "4000000000009995") == "9995"
    }

    test "an invalid number has no last four digits" {
        expect TestCards.lastFour(of: "not a card") == ""
    }

    // The Luhn check itself

    test "the Luhn check doubles every second digit from the right" {
        expect TestCards.passesLuhn([7, 9, 9, 2, 7, 3, 9, 8, 7, 1, 3])
        expect not TestCards.passesLuhn([7, 9, 9, 2, 7, 3, 9, 8, 7, 1, 4])
    }
}
