import k.crypto.Hmac

/// A parsed `Stripe-Signature` header, and the checks Stripe documents for
/// it.
///
/// The header looks like `t=1700000000,v1=5257a869…,v1=…`. Stripe signs
/// `"{t}.{raw body}"` with HMAC-SHA256 under the endpoint's signing secret
/// and sends the hex digest as `v1`. There may be several `v1` entries while
/// a secret is being rolled, and any one may match. Other schemes (such as
/// `v0`, used only in test mode) are ignored.
///
/// This type has no dependencies: it takes the current time and the secret
/// as arguments, so every edge case can be unit tested without a clock or a
/// config.
public struct StripeSignature {
    /// When Stripe signed the payload, from the `t` entry.
    let signedAt: Date

    /// The decoded `v1` signatures.
    let candidates: List<Bytes>

    /// Parses a `Stripe-Signature` header.
    ///
    /// - Parameter header: the header's value.
    /// - Returns: the signature, or `nil` when the header has no valid `t`
    ///   entry or no well-formed `v1` entry.
    static func parse(header: String) -> StripeSignature? {
        var timestamp: Int? = nil
        var candidates: List<Bytes> = []

        for entry in header.split(separator: ",") {
            let parts = entry.split(separator: "=", limit: 2)

            guard parts.count == 2 else {
                continue
            }

            let value = parts[1].trimmed()

            match parts[0].trimmed() {
                case "t" {
                    timestamp = Int.parse(value)
                }

                case "v1" {
                    if let digest = Bytes.parse(hex: value) {
                        candidates.append(digest)
                    }
                }

                default {
                    continue
                }
            }
        }

        guard let timestamp, candidates.isNotEmpty else {
            return nil
        }

        return StripeSignature(signedAt: Date(unixSeconds: timestamp), candidates: candidates)
    }

    /// Whether the signature was made recently enough to accept.
    ///
    /// - Parameters:
    ///   - now: the current time.
    ///   - tolerance: how far `signedAt` may be from `now`, in either
    ///     direction. Allowing a little future skew tolerates clock drift
    ///     between Stripe and this server.
    func isFresh(now: Date, tolerance: Duration) -> Bool {
        return signedAt >= now - tolerance and signedAt <= now + tolerance
    }

    /// Whether any `v1` signature is the HMAC-SHA256 of the payload.
    ///
    /// The digest is a `k.crypto.Mac`, which can't be compared with `==`. Its
    /// `matches` takes the same time however many leading bytes match, so
    /// response timing can't be used to guess a valid signature byte by
    /// byte. It is `async` because HMAC runs on the platform's Web Crypto.
    ///
    /// - Parameters:
    ///   - payload: the raw request body, exactly as received. Re-encoding
    ///     parsed JSON would change the bytes and break the signature.
    ///   - secret: the endpoint's signing secret. `Hmac` takes it as a
    ///     `Secret`, so it is never revealed here.
    func matches(payload: Bytes, secret: Secret<String>) async -> Bool {
        let signedPayload = "{signedAt.unixSeconds}.".utf8 + payload
        let expected = await Hmac.sha256(key: secret, message: signedPayload)

        return candidates.contains { candidate => expected.matches(candidate) }
    }
}
