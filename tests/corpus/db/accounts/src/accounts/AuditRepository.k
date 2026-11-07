/// The audit log.
public repository AuditRepository for Audit {
    /// The messages, in order.
    func messages() async -> List<String> {
        let all = await audits.sortedBy { audit => audit.message }

        return all.map { audit => audit.message }
    }
}
