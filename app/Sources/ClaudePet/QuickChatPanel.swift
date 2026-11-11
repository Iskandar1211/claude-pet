import AppKit
import SwiftUI

/// Окно Quick Chat. В отличие от окна питомца может стать key (нужен ввод с клавиатуры), но панель
/// `.nonactivatingPanel` не активирует приложение — фокус у других окон не отбирается, Dock-значка нет (LSUIElement).
final class QuickChatPanel: NSPanel {
    static let size = NSSize(width: 420, height: 560)
    static let minSize = NSSize(width: 320, height: 400)

    /// Esc закрывает панель, откуда бы он ни пришёл (в том числе из поля ввода).
    var onCancel: (() -> Void)?

    init(content: NSView) {
        super.init(
            contentRect: NSRect(origin: .zero, size: Self.size),
            styleMask: [.borderless, .resizable, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        minSize = Self.minSize
        isFloatingPanel = true
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        hidesOnDeactivate = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isMovableByWindowBackground = true
        contentView = content
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

/// Открывает/закрывает панель рядом с питомцем.
@MainActor
final class QuickChatController {
    private let model: QuickChatModel
    private let panel: QuickChatPanel
    private var pasteMonitor: Any?

    init(model: QuickChatModel) {
        self.model = model
        // Кнопка закрытия во view и Esc в панели — одно действие; self здесь ещё не готов, поэтому через переменную.
        var close: () -> Void = {}
        panel = QuickChatPanel(content: NSHostingView(rootView: QuickChatView(model: model, onClose: { close() })))
        close = { [weak self] in self?.close() }
        panel.onCancel = close
    }

    func toggle(near pet: NSWindow) {
        panel.isVisible ? close() : open(near: pet)
    }

    /// Открыть (если закрыта) рядом с питомцем; уже открытая остаётся как есть.
    func show(near pet: NSWindow) {
        if !panel.isVisible { open(near: pet) }
    }

    private func open(near pet: NSWindow) {
        panel.setFrameOrigin(origin(near: pet.frame))
        panel.makeKeyAndOrderFront(nil)
        installPasteMonitor()
        model.panelOpened()
    }

    private func close() {
        panel.orderOut(nil)
        if let pasteMonitor { NSEvent.removeMonitor(pasteMonitor) }
        pasteMonitor = nil
        model.panelClosed()
    }

    /// У приложения нет меню «Правка», поэтому ⌘V ловим сами и только для нетекстового буфера (файлы, картинка);
    /// с текстом событие уходит дальше без изменений. Пока открыт лист выбора файла, событие идёт в него, а не в панель.
    private func installPasteMonitor() {
        if let pasteMonitor { NSEvent.removeMonitor(pasteMonitor) }
        pasteMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            nonisolated(unsafe) let event = event // монитор вызывается в главном потоке
            let consumed = MainActor.assumeIsolated { self?.handlePaste(event) ?? false }
            return consumed ? nil : event
        }
    }

    private func handlePaste(_ event: NSEvent) -> Bool {
        guard event.window === panel,
              event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command,
              event.charactersIgnoringModifiers == "v",
              model.pasteAttachments()
        else { return false }
        return true
    }

    /// Над питомцем, по центру; если сверху не помещается — под ним. Всегда внутри видимой области экрана.
    private func origin(near pet: NSRect) -> NSPoint {
        let size = panel.frame.size
        let gap: CGFloat = 8
        let center = NSPoint(x: pet.midX, y: pet.midY)
        let area = (NSScreen.screens.first { $0.frame.contains(center) } ?? NSScreen.main)?.visibleFrame ?? pet
        var y = pet.maxY + gap
        if y + size.height > area.maxY { y = pet.minY - size.height - gap }
        let x = pet.midX - size.width / 2
        return NSPoint(
            x: min(max(x, area.minX), area.maxX - size.width),
            y: min(max(y, area.minY), area.maxY - size.height)
        )
    }
}
