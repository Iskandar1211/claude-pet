import AppKit

/// Окно питомца: маленькая панель размером с персонажа. Видна поверх всех окон,
/// на всех Spaces и над полноэкранными приложениями; фокус у активного приложения не забирает.
/// Ходьба (шаг 3b) — это перемещение самой панели, поэтому клики сквозь окно не нужны.
final class PetPanel: NSPanel {
    init(content: NSView) {
        super.init(
            contentRect: NSRect(origin: .zero, size: content.frame.size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        // Выше Dock и строки меню; .floating (как alwaysOnTop) ниже них.
        level = .statusBar
        // Без .fullScreenAuxiliary macOS не пускает чужое окно в Space полноэкранного приложения.
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        hidesOnDeactivate = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        contentView = content
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}
