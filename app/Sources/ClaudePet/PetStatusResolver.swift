import Foundation

/// Статус питомца по всем сессиям. Приоритет:
/// ждёт одобрения → ошибка → свежее непросмотренное «Готово» → работает → старое «Готово» → ожидание.
///
/// В отличие от `pet_status` моста «Готово» перебивает работающую сессию только пока оно свежее
/// (`freshDoneFor`) и непросмотренное: иначе давно закончившая сессия навсегда держала бы питомца на «Готово».
/// Единственная завершённая сессия остаётся на «Готово», как и раньше.
struct PetStatusResolver {
    static let freshDoneFor: TimeInterval = 60

    private var previous: [String: PetStatus]?
    /// Когда сессия перешла в done на наших глазах; `.distantPast` — уже была done, когда мы её увидели.
    private var doneSince: [String: Date] = [:]
    private var seen: Set<String> = []

    /// `viewing` — сессия, чей чат открыт сейчас: её «Готово» считается просмотренным.
    mutating func resolve(_ sessions: [BridgeSession], now: Date, viewing: String?) -> PetStatus {
        for session in sessions {
            let id = session.sessionId
            if session.status == .done {
                if doneSince[id] == nil {
                    let justFinished = previous?[id].map { $0 != .done } ?? false
                    doneSince[id] = justFinished ? now : .distantPast
                }
                if viewing == id { seen.insert(id) }
            } else {
                doneSince[id] = nil
                seen.remove(id)
            }
        }
        let ids = Set(sessions.map(\.sessionId))
        doneSince = doneSince.filter { ids.contains($0.key) }
        seen.formIntersection(ids)
        previous = Dictionary(sessions.map { ($0.sessionId, $0.status) }, uniquingKeysWith: { $1 })

        func any(_ status: PetStatus) -> Bool { sessions.contains { $0.status == status } }
        let freshDone = sessions.contains { session in
            guard session.status == .done, !seen.contains(session.sessionId),
                  let since = doneSince[session.sessionId] else { return false }
            return now.timeIntervalSince(since) < Self.freshDoneFor
        }
        if any(.waiting) { return .waiting }
        if any(.error) { return .error }
        if freshDone { return .done }
        if any(.working) { return .working }
        if any(.done) { return .done }
        return .idle
    }
}
