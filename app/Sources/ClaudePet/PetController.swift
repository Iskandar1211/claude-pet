import AppKit

/// Поведение питомца по образцу WindowPet (https://github.com/SeakMengs/WindowPet, src/scenes/Pets.ts),
/// но двигается само окно: падение, ходьба с разворотом у края экрана, перетаскивание и бросок.
/// Статус, отличный от idle, держит Анну на месте в анимации статуса — как setStatus() в Pets.ts.
@MainActor
final class PetController {
    let panel: PetPanel
    private let scene: PetScene
    private let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion

    private static let gravity: CGFloat = 1800      // pt/s²
    private static let walkSpeed: CGFloat = 45      // pt/s
    private static let maxThrowSpeed: CGFloat = 1500
    private static let wallBounce: CGFloat = 0.4
    /// Медленнее этого при отпускании питомец считается «поставленным» и остаётся на месте, быстрее — брошенным.
    private static let placeSpeed: CGFloat = 400    // pt/s
    /// Если мышь стояла дольше, скорость руки в момент отпускания — ноль (последнее движение уже не в счёт).
    private static let holdStillAfter: TimeInterval = 0.1

    /// Клик по питомцу без перетаскивания.
    var onClick: (() -> Void)?

    private var status: PetStatus = .idle
    private var velocity = CGVector.zero            // pt/s, оси Cocoa (y вверх)
    private var walkDirection: CGFloat = 0          // -1 влево, 0 стоит, +1 вправо
    private var nextDecision: TimeInterval = 0
    private var lastTime: TimeInterval?

    /// Сдвиг мыши, после которого нажатие считается перетаскиванием, а не кликом.
    private static let dragThreshold: CGFloat = 3
    private static let savedOriginKey = "petOrigin"
    private static let savedPerchedKey = "petPerched"

    private var pressPoint: NSPoint?
    private var isDragging = false
    private var grabOffset = CGVector.zero
    private var lastDrag: (point: NSPoint, time: TimeInterval)?
    private var dragVelocity = CGVector.zero
    /// Поставлен выше пола и стоит на месте: без гравитации и прогулок, пока его не возьмут снова.
    private var isPerched = false

    init(panel: PetPanel, scene: PetScene) {
        self.panel = panel
        self.scene = scene
        scene.onUpdate = { [weak self] time in self?.step(time) }
        refreshAnimation()
    }

    /// Позиция с прошлого запуска (как у ChatGPT Pets); если её нет или того экрана уже нет —
    /// как в WindowPet: появляется сверху у правого края и падает.
    func spawn() {
        if let saved = UserDefaults.standard.string(forKey: Self.savedOriginKey).map(NSPointFromString),
           NSScreen.screens.contains(where: { $0.visibleFrame.contains(saved) }) {
            panel.setFrameOrigin(saved)
            isPerched = UserDefaults.standard.bool(forKey: Self.savedPerchedKey)
        } else if let bounds = screenBounds() {
            panel.setFrameOrigin(NSPoint(x: bounds.maxX - panel.frame.width - 24, y: bounds.maxY - panel.frame.height))
        }
        panel.orderFrontRegardless()
        refreshAnimation()
    }

    func savePosition() {
        UserDefaults.standard.set(NSStringFromPoint(panel.frame.origin), forKey: Self.savedOriginKey)
        UserDefaults.standard.set(isPerched, forKey: Self.savedPerchedKey)
    }

    func setStatus(_ newStatus: PetStatus) {
        // Снимок моста приходит при любом изменении (например, новом сообщении) — тот же статус не сбрасывает прогулку.
        guard newStatus != status else { return }
        status = newStatus
        walkDirection = 0
        nextDecision = 0 // в idle сразу выбрать, что делать дальше
        refreshAnimation()
    }

    // MARK: - Перетаскивание (координаты экрана, NSEvent.mouseLocation)

    func mouseDown(at point: NSPoint, time: TimeInterval) {
        pressPoint = point
        dragVelocity = .zero
        grabOffset = CGVector(dx: point.x - panel.frame.minX, dy: point.y - panel.frame.minY)
        lastDrag = (point, time)
    }

    func mouseDragged(to point: NSPoint, time: TimeInterval) {
        guard let press = pressPoint else { return }
        if !isDragging {
            // Дрожание руки при клике не должно включать позу перетаскивания.
            guard hypot(point.x - press.x, point.y - press.y) >= Self.dragThreshold else { return }
            isDragging = true
            velocity = .zero
            refreshAnimation()
        }
        if let last = lastDrag, time > last.time {
            let dt = CGFloat(time - last.time)
            dragVelocity = CGVector(dx: (point.x - last.point.x) / dt, dy: (point.y - last.point.y) / dt)
        }
        lastDrag = (point, time)
        panel.setFrameOrigin(NSPoint(x: point.x - grabOffset.dx, y: point.y - grabOffset.dy))
    }

    /// Клик без перетаскивания — открыть/закрыть Quick Chat. После перетаскивания: бросок (быстрая рука) —
    /// питомец падает; поставили (медленная) выше пола — остаётся на месте.
    func mouseUp(time: TimeInterval) {
        pressPoint = nil
        guard isDragging else {
            onClick?()
            return
        }
        isDragging = false
        let heldStill = lastDrag.map { time - $0.time > Self.holdStillAfter } ?? true
        let thrown = heldStill ? .zero : dragVelocity
        lastDrag = nil
        let aboveFloor = screenBounds().map { panel.frame.minY > $0.minY + 0.5 } ?? false
        if aboveFloor, hypot(thrown.dx, thrown.dy) < Self.placeSpeed {
            isPerched = true
            velocity = .zero
            walkDirection = 0
            savePosition()
        } else {
            isPerched = false
            velocity = CGVector(dx: clampThrow(thrown.dx), dy: clampThrow(thrown.dy))
        }
        refreshAnimation()
    }

    // MARK: - Кадр

    private func step(_ time: TimeInterval) {
        let dt = CGFloat(min(time - (lastTime ?? time), 1.0 / 20)) // после паузы не «телепортируемся»
        lastTime = time
        guard !isDragging, let bounds = screenBounds() else { return }
        if isPerched {
            // Если пол оказался под ней (сменился экран или Dock), снова обычная физика.
            guard panel.frame.minY <= bounds.minY + 0.5 else { return }
            isPerched = false
        }

        var origin = panel.frame.origin
        let size = panel.frame.size
        let wasAirborne = origin.y > bounds.minY + 0.5

        if wasAirborne {
            velocity.dy -= Self.gravity * dt
        } else {
            velocity = CGVector(dx: walkDirection * Self.walkSpeed, dy: 0)
        }
        origin.x += velocity.dx * dt
        origin.y += velocity.dy * dt

        // Стены: в полёте отскок, на земле — разворот (лазанья по стенам у Анны нет).
        if origin.x < bounds.minX || origin.x > bounds.maxX - size.width {
            origin.x = min(max(origin.x, bounds.minX), bounds.maxX - size.width)
            if wasAirborne {
                velocity.dx = -velocity.dx * Self.wallBounce
            } else if walkDirection != 0 {
                walkDirection = origin.x <= bounds.minX ? 1 : -1
                refreshAnimation()
            }
        }
        if origin.y > bounds.maxY - size.height {
            origin.y = bounds.maxY - size.height
            velocity.dy = min(velocity.dy, 0)
        }
        if origin.y <= bounds.minY {
            origin.y = bounds.minY
            if wasAirborne { land() }
        }
        if origin != panel.frame.origin { panel.setFrameOrigin(origin) }

        if !wasAirborne { decide(time) }
    }

    private func land() {
        velocity = .zero
        refreshAnimation()
        savePosition() // и при выходе — см. AppDelegate; здесь — на случай падения приложения
    }

    /// Случайное поведение только в idle, как playRandomState() в WindowPet: постоять или пройтись.
    private func decide(_ time: TimeInterval) {
        guard status == .idle, !reduceMotion, time >= nextDecision else { return }
        nextDecision = time + .random(in: 3...7)
        walkDirection = Bool.random() ? 0 : (Bool.random() ? 1 : -1)
        refreshAnimation()
    }

    private func refreshAnimation() {
        if walkDirection != 0 { scene.facingLeft = walkDirection < 0 }
        let airborne = isDragging || (!isPerched && panel.frame.minY > (screenBounds()?.minY ?? 0) + 0.5)
        let state: String
        if airborne {
            state = "drag"
        } else if status != .idle {
            state = status.rawValue
        } else {
            state = walkDirection != 0 ? "walk" : "idle"
        }
        scene.play(state, animated: !reduceMotion)
    }

    // MARK: - Экран

    /// Видимая область экрана, на котором сейчас центр питомца (без строки меню и Dock).
    private func screenBounds() -> NSRect? {
        let center = NSPoint(x: panel.frame.midX, y: panel.frame.midY)
        let screen = NSScreen.screens.first { $0.frame.contains(center) } ?? NSScreen.main
        return screen?.visibleFrame
    }

    private func clampThrow(_ v: CGFloat) -> CGFloat {
        max(-Self.maxThrowSpeed, min(Self.maxThrowSpeed, v))
    }
}
