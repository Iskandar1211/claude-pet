import Foundation
import UserNotifications

/// Переход статуса отдельной сессии, о котором стоит сказать пользователю.
struct SessionTransition: Equatable {
    let session: BridgeSession
    let status: PetStatus
}

/// Чистая логика: что изменилось между снимками. `previous == nil` — первый снимок, молчим
/// (иначе при старте посыплются уведомления по старым сессиям).
/// `waiting` — уведомляем и для новой сессии; `done` — только если сессия уже была и статус сменился.
func sessionTransitions(previous: [String: PetStatus]?, current: [BridgeSession]) -> [SessionTransition] {
    guard let previous else { return [] }
    return current.compactMap { session in
        let old = previous[session.sessionId]
        switch session.status {
        case .waiting where old != .waiting: return SessionTransition(session: session, status: .waiting)
        case .done where old != nil && old != .done: return SessionTransition(session: session, status: .done)
        default: return nil
        }
    }
}

/// Текст уведомления. В него идут только имя папки, источник и tool_name (всё очищено);
/// last_message / input_preview недоверенные — не вставляем.
struct NotificationPlan: Equatable {
    let title: String
    let subtitle: String
    let body: String
}

/// Короткие задачи не стоят уведомления: «Claude закончил» — только после работы дольше этого порога.
let doneNotificationMinWork: TimeInterval = 20

/// Что показать для перехода, либо `nil`, если молчим.
/// - `worked`: сколько сессия была занята до этого перехода (`nil` — начала не видели).
/// - `chatOpen`: Quick Chat уже открыт на этой сессии — пользователь и так смотрит на неё.
func notificationPlan(for transition: SessionTransition, worked: TimeInterval?, chatOpen: Bool) -> NotificationPlan? {
    guard !chatOpen else { return nil }
    let session = transition.session
    switch transition.status {
    case .waiting:
        let tool = session.pendingPermission.map { "Инструмент: \(UntrustedText.clean($0.toolName, limit: 60))" }
        return NotificationPlan(
            title: "\(session.displayName) — ждёт одобрения",
            subtitle: session.sourceTitle,
            body: tool ?? "Нужно твоё одобрение")
    case .done:
        guard let worked, worked >= doneNotificationMinWork else { return nil }
        return NotificationPlan(
            title: "\(session.displayName) — закончил",
            subtitle: session.sourceTitle,
            body: "Работал \(workedDescription(worked))")
    default:
        return nil
    }
}

/// «35 с», «2 мин», «2 мин 5 с».
func workedDescription(_ seconds: TimeInterval) -> String {
    let total = Int(seconds.rounded())
    guard total >= 60 else { return "\(total) с" }
    let (minutes, rest) = total.quotientAndRemainder(dividingBy: 60)
    return rest == 0 ? "\(minutes) мин" : "\(minutes) мин \(rest) с"
}

/// Системные уведомления о переходах в `waiting` и `done`. Клик по уведомлению открывает чат этой сессии.
@MainActor
final class SessionNotifier: NSObject, UNUserNotificationCenterDelegate {
    private nonisolated static let sessionKey = "sessionId"

    private var known: [String: PetStatus]?
    /// Когда сессия перешла из покоя в работу/ожидание: от этого считается «работал N».
    private var activeSince: [String: Date] = [:]

    /// Quick Chat открыт именно на этой сессии?
    var isChatOpen: ((String) -> Bool)?
    /// Клик по уведомлению.
    var onOpen: ((String) -> Void)?

    /// Разрешение просим при запуске. При ad-hoc подписи после пересборки macOS может спросить снова.
    func requestAuthorization() {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.requestAuthorization(options: [.alert, .sound]) { _, error in
            if let error { NSLog("ClaudePet: разрешение на уведомления: \(error)") }
        }
    }

    func update(_ sessions: [BridgeSession], now: Date = Date()) {
        let transitions = sessionTransitions(previous: known, current: sessions)
        for session in sessions where session.status == .working || session.status == .waiting {
            if activeSince[session.sessionId] == nil { activeSince[session.sessionId] = now }
        }
        for transition in transitions {
            let id = transition.session.sessionId
            let worked = activeSince[id].map { now.timeIntervalSince($0) }
            guard let plan = notificationPlan(
                for: transition, worked: worked, chatOpen: isChatOpen?(id) ?? false) else { continue }
            post(plan, sessionId: id, status: transition.status)
        }
        // Задача закончилась (или сессия исчезла) — следующая отсчитывается заново.
        let active = Set(sessions.filter { $0.status == .working || $0.status == .waiting }.map(\.sessionId))
        activeSince = activeSince.filter { active.contains($0.key) }
        known = Dictionary(sessions.map { ($0.sessionId, $0.status) }, uniquingKeysWith: { $1 })
    }

    private func post(_ plan: NotificationPlan, sessionId: String, status: PetStatus) {
        let content = UNMutableNotificationContent()
        content.title = plan.title
        content.subtitle = plan.subtitle
        content.body = plan.body
        content.sound = .default
        content.threadIdentifier = sessionId
        content.userInfo = [Self.sessionKey: sessionId]
        let request = UNNotificationRequest(identifier: "\(sessionId)-\(status.rawValue)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request) { error in
            if let error { NSLog("ClaudePet: не удалось показать уведомление: \(error)") }
        }
    }

    // MARK: - UNUserNotificationCenterDelegate

    /// Приложение-аксессуар почти никогда не активно, но если вдруг активно — баннер всё равно нужен.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping @Sendable (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping @Sendable () -> Void
    ) {
        let id = response.notification.request.content.userInfo[Self.sessionKey] as? String
        Task { @MainActor in
            if let id { self.onOpen?(id) }
            completionHandler()
        }
    }
}
