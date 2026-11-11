import AppKit
import SwiftUI

/// Окно пузыря одобрения: как окно питомца — не активирует приложение, поверх всего, на всех Spaces.
/// Key не становится: клавиатура у активного приложения не отбирается, но по кнопкам кликать можно.
final class ApprovalBubblePanel: NSPanel {
    init(content: NSView) {
        super.init(
            contentRect: NSRect(origin: .zero, size: content.frame.size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        hidesOnDeactivate = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        contentView = content
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Хост, который сообщает об изменении высоты содержимого (появилась ошибка и т. п.) и принимает первый клик.
private final class BubbleHost: NSHostingView<ApprovalBubbleView> {
    var onResize: (() -> Void)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func invalidateIntrinsicContentSize() {
        super.invalidateIntrinsicContentSize()
        DispatchQueue.main.async { [weak self] in self?.onResize?() }
    }
}

struct ApprovalBubbleView: View {
    static let width: CGFloat = 320
    let model: QuickChatModel

    var body: some View {
        if let bubble = model.bubble {
            ApprovalCard(
                model: model, session: bubble.session, pending: bubble.pending, compact: true,
                onDismiss: { model.dismissBubble(requestId: bubble.pending.requestId) },
                more: bubble.more, armDelay: .milliseconds(400)
            )
            .id(bubble.pending.requestId) // следующий запрос — новая карточка со своей задержкой кнопок
            .background(ChatTheme.panelBackground)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(ChatTheme.panelBorder))
            .environment(\.colorScheme, .dark)
            .foregroundStyle(ChatTheme.text)
            .frame(width: Self.width)
        }
    }
}

/// Показывает пузырь над питомцем, когда есть запрос (`model.bubble`), и прячет, когда его нет.
/// Позицию обновляет `follow()` — его зовёт питомец при движении; своего таймера нет.
@MainActor
final class ApprovalBubbleController {
    private let model: QuickChatModel
    private let pet: NSWindow
    private let panel: ApprovalBubblePanel
    private let host: BubbleHost

    init(model: QuickChatModel, pet: NSWindow) {
        self.model = model
        self.pet = pet
        host = BubbleHost(rootView: ApprovalBubbleView(model: model))
        host.frame = NSRect(x: 0, y: 0, width: ApprovalBubbleView.width, height: 10)
        panel = ApprovalBubblePanel(content: host)
        host.onResize = { [weak self] in self?.follow() }
        observe()
    }

    /// Перерисовка пузыря идёт сама (SwiftUI следит за моделью); здесь — только показать/спрятать окно.
    private func observe() {
        withObservationTracking {
            _ = model.bubble?.pending.requestId
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.follow()
                self?.observe()
            }
        }
    }

    /// Окно должно быть видно, только пока есть что показывать и сам питомец на экране.
    func follow() {
        guard model.bubble != nil, pet.isVisible else {
            if panel.isVisible { panel.orderOut(nil) }
            return
        }
        let size = host.fittingSize
        guard size.width > 0, size.height > 0 else { return }
        let origin = origin(for: size)
        panel.setFrame(NSRect(origin: origin, size: size), display: true)
        if !panel.isVisible { panel.orderFrontRegardless() }
    }

    /// Над питомцем по центру; нет места сверху — под ним. Всегда внутри видимой области его экрана.
    private func origin(for size: NSSize) -> NSPoint {
        let pet = pet.frame
        let gap: CGFloat = 4
        let center = NSPoint(x: pet.midX, y: pet.midY)
        let area = (NSScreen.screens.first { $0.frame.contains(center) } ?? NSScreen.main)?.visibleFrame ?? pet
        var y = pet.maxY + gap
        if y + size.height > area.maxY { y = pet.minY - size.height - gap }
        return NSPoint(
            x: min(max(pet.midX - size.width / 2, area.minX), area.maxX - size.width),
            y: min(max(y, area.minY), area.maxY - size.height)
        )
    }
}
