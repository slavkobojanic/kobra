import k.crypto.Uuid

/// Queries for `Membership`.
public repository MembershipRepository for Membership {
    /// `member`'s membership of `room`, or `nil`.
    func find(room: String, member: String) async -> Membership? {
        return await memberships.first { m => m.room == room and m.member == member }
    }

    /// Every room `member` joined.
    func of(member: String) async -> List<Membership> {
        return await memberships
            .filter { m => m.member == member }
            .list()
    }

    /// Makes `member` a member of `room`, unless they are one already (a
    /// second tab joining at the same moment): the membership's unique
    /// `key` lets the first insert win.
    func join(room: String, member: String, moderator: Bool) async {
        do {
            try await insert(Membership(id: Uuid.random(), key: "{room} {member}", room: room, member: member, lastRead: 0, moderator: moderator, removed: false))
        } catch .key {
            // Already a member.
        }
    }

    /// Whether `member` is in `room` now: joined, and not removed.
    func isMember(room: String, member: String) async -> Bool {
        guard let membership = await find(room: room, member: member) else {
            return false
        }

        return not membership.removed
    }

    /// The moderators of `room`, by name.
    func moderators(room: String) async -> List<String> {
        let found = await memberships
            .filter { m => m.room == room and m.moderator }
            .sortedBy { m => m.member }
            .list()

        return found.map { m => m.member }
    }

    /// Moves `member`'s place in `room` forward to `upTo`, never back.
    ///
    /// - Returns: `true` if it moved.
    func markRead(room: String, member: String, upTo: Int) async -> Bool {
        let updated = await memberships
            .filter { m => m.room == room and m.member == member and m.lastRead < upTo }
            .update { m => m.lastRead = upTo }

        return updated == 1
    }

    /// Makes a member of `room` a moderator.
    ///
    /// - Returns: `false` if `member` isn't a member.
    func promote(room: String, member: String) async -> Bool {
        let updated = await memberships
            .filter { m => m.room == room and m.member == member and not m.removed }
            .update { m => m.moderator = true }

        return updated == 1
    }

    /// Removes `member` from `room` for good.
    func remove(room: String, member: String) async {
        await memberships
            .filter { m => m.room == room and m.member == member }
            .update { m => m.removed = true }
    }
}
