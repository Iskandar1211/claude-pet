import AppKit
import SpriteKit

/// Содержимое окна питомца: SpriteKit-вид с прозрачным фоном.
/// Мышь отдаёт в PetController — он двигает окно и считает скорость броска.
final class PetView: SKView {
    /// Размер на экране, pt.
    static let displaySize: CGFloat = 128

    weak var controller: PetController?

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: Self.displaySize, height: Self.displaySize))
        allowsTransparency = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) не используется") }

    // Клик срабатывает сразу, без активации приложения.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        controller?.mouseDown(at: NSEvent.mouseLocation, time: event.timestamp)
    }

    override func mouseDragged(with event: NSEvent) {
        controller?.mouseDragged(to: NSEvent.mouseLocation, time: event.timestamp)
    }

    override func mouseUp(with event: NSEvent) {
        controller?.mouseUp(time: event.timestamp)
    }
}
