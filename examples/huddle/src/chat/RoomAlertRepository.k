import k.device.PushSubscription

/// Queries for `RoomAlert`.
public repository RoomAlertRepository for RoomAlert {
    /// Every alert of `room`.
    func of(room: String) async -> List<RoomAlert> {
        return await roomAlerts
            .filter { alert => alert.room == room }
            .list()
    }

    /// The alert of `room` for the browser at `endpoint`, or `nil`.
    func find(room: String, endpoint: Url) async -> RoomAlert? {
        return await roomAlerts.first { alert => alert.room == room and alert.endpoint == endpoint }
    }

    /// Deletes every alert of `room` for `member`.
    func deleteAll(room: String, member: String) async {
        await roomAlerts
            .filter { alert => alert.room == room and alert.member == member }
            .delete()
    }

    /// The subscription `alert` sends to.
    func subscription(of alert: RoomAlert) -> PushSubscription {
        return PushSubscription(endpoint: alert.endpoint, p256dh: alert.p256dh, auth: alert.auth)
    }
}
