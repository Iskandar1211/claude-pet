import AppKit

/// Капсула под ногами питомца: сколько процентов 5-часового лимита Claude израсходовано
/// (событие `usage`, bridge/PROTOCOL.md). При наведении — время сброса.
/// Данные приходят только из терминальных сессий, поэтому старые сереют, а после сброса окна капсула
/// показывает «—» (лимит неизвестен до нового запроса в терминале). Клик по капсуле перечитывает данные.
@MainActor
final class UsageBadge: NSView {
    static let height: CGFloat = 18
    /// Без терминала `usage.json` не обновляется — старый процент быстро сереет.
    private static let staleAfter: TimeInterval = 5 * 60
    private static let padding: CGFloat = 7

    /// Капсула появилась или скрылась — окну питомца нужно поменять высоту.
    var onVisibilityChange: ((Bool) -> Void)?
    /// Клик по капсуле: перечитать `usage.json` сейчас, не дожидаясь опроса.
    var onRefreshRequested: (() -> Void)?

    private let label = NSTextField(labelWithString: "")
    private var usage: BridgeClient.Usage?
    private var hovering = false
    /// Короткий ответ на клик («обновлено ✓» или возраст данных) поверх процента.
    private var flash: (text: String, until: Date)?

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 0, height: Self.height))
        wantsLayer = true
        layer?.cornerRadius = Self.height / 2
        label.font = .monospacedDigitSystemFont(ofSize: 11, weight: .semibold)
        label.alignment = .center
        label.isEditable = false
        label.isBezeled = false
        label.isBordered = false
        label.drawsBackground = false
        label.isSelectable = false
        addSubview(label)
        isHidden = true
        // Сереть и скрываться капсула должна и без новых событий.
        Task { [weak self] in
            while true {
                try? await Task.sleep(for: .seconds(30))
                guard let self else { return }
                self.refresh()
            }
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) не используется") }

    func update(_ usage: BridgeClient.Usage) {
        self.usage = usage
        refresh()
    }

    // Клик ловит сама капсула (остальное окно питомца не затронуто); наведение — tracking area.
    override func hitTest(_ point: NSPoint) -> NSView? { !isHidden && frame.contains(point) ? self : nil }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        let before = usage?.updatedAt
        onRefreshRequested?()
        let text = usage?.updatedAt != before
            ? "обновлено ✓"
            : (usage.map { UsageStore.ageText(from: $0.updatedAt) } ?? "нет данных")
        flash = (text, Date().addingTimeInterval(2.5))
        refresh()
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(2.6))
            self?.refresh()
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        // .activeAlways: приложение-аксессуар почти никогда не активно.
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseEntered(with event: NSEvent) {
        hovering = true
        refresh()
    }

    override func mouseExited(with event: NSEvent) {
        hovering = false
        refresh()
    }

    private func refresh() {
        let now = Date()
        guard let usage else {
            setVisible(false)
            return
        }
        // Окно сброшено: процент устарел и неизвестно, сколько израсходовано в новом.
        let expired = usage.resetsAt <= now
        let stale = now.timeIntervalSince(usage.updatedAt) > Self.staleAfter
        let pct = usage.fiveHourPct
        let background: NSColor, text: NSColor
        switch (stale || expired, pct) {
        case (true, _): (background, text) = (.systemGray.withAlphaComponent(0.6), .white.withAlphaComponent(0.8))
        case (_, 90...): (background, text) = (.systemRed, .white)
        case (_, 70...): (background, text) = (.systemYellow, .black)
        default: (background, text) = (.black.withAlphaComponent(0.55), .white)
        }
        layer?.backgroundColor = background.cgColor
        label.textColor = text
        if let flash, flash.until > now {
            label.stringValue = flash.text
        } else if expired {
            label.stringValue = "—"
        } else {
            label.stringValue = hovering
                ? "сброс \(usage.resetsAt.formatted(date: .omitted, time: .shortened))"
                : "\(Int(pct.rounded()))%"
        }
        let age = UsageStore.ageText(from: usage.updatedAt, now: now)
        let hint = "Клик — перечитать. Данные приходят из терминала Claude."
        toolTip = expired
            ? "Лимит неизвестен: окно сброшено в \(usage.resetsAt.formatted(date: .omitted, time: .shortened)). Отправь запрос в терминале Claude — данные обновятся. \(hint)"
            : "Лимит Claude за 5 часов: \(Int(pct.rounded()))% (обновлено \(age)). \(hint)"

        setVisible(true)
        layoutInChrome()
    }

    /// Вызывается после изменения высоты chrome (см. `onVisibilityChange`).
    func layoutInChrome() {
        guard let parent = superview, !isHidden else { return }
        label.sizeToFit()
        let width = max(label.frame.width + Self.padding * 2, 34)
        let y = parent.isFlipped ? (parent.bounds.height - Self.height) : 0
        frame = NSRect(x: ((parent.bounds.width - width) / 2).rounded(), y: y, width: width, height: Self.height)
        label.frame = NSRect(
            x: Self.padding,
            y: ((Self.height - label.frame.height) / 2).rounded(),
            width: label.frame.width,
            height: label.frame.height
        )
    }

    private func setVisible(_ visible: Bool) {
        let shouldHide = !visible
        guard isHidden != shouldHide else { return }
        isHidden = shouldHide
        onVisibilityChange?(visible)
    }
}
