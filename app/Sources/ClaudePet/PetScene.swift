import SpriteKit

/// Сцена размером с окно питомца: один спрайт Анны и смена анимаций.
/// Физики SpriteKit здесь нет — двигается само окно (см. PetController).
final class PetScene: SKScene {
    /// Вызывается каждый кадр отрисовки — по нему PetController двигает окно.
    var onUpdate: (@MainActor (TimeInterval) -> Void)?

    var facingLeft = false {
        didSet { sprite.xScale = facingLeft ? -1 : 1 }
    }

    private let sheet: SpriteSheet
    private let sprite = SKSpriteNode()
    private var currentState: String?

    init(sheet: SpriteSheet, size: CGSize) {
        self.sheet = sheet
        super.init(size: size)
        backgroundColor = .clear
        sprite.size = size
        sprite.position = CGPoint(x: size.width / 2, y: size.height / 2)
        addChild(sprite)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) не используется") }

    /// `animated = false` — статичный первый кадр (системная настройка «Уменьшение движения»).
    func play(_ state: String, animated: Bool) {
        guard state != currentState else { return }
        let textures = sheet.textures(for: state)
        guard let first = textures.first else { return }
        currentState = state
        sprite.removeAction(forKey: "anim")
        sprite.texture = first
        if animated, textures.count > 1 {
            sprite.run(Self.animation(state, textures), withKey: "anim")
        }
    }

    override func update(_ currentTime: TimeInterval) {
        onUpdate?(currentTime)
    }

    /// В листе по 4 кадра на состояние (anna.json). Кадры — не равномерный цикл, а маленькие сценки:
    /// моргание, «печатает, потом задумалась», «ликует, потом успокоилась».
    private static func animation(_ state: String, _ textures: [SKTexture]) -> SKAction {
        // Если в листе кадров меньше, берём последний — сцена не падает на старом листе.
        func frame(_ index: Int) -> SKTexture { textures[min(index, textures.count - 1)] }
        func show(_ index: Int, _ seconds: TimeInterval) -> SKAction {
            .sequence([.setTexture(frame(index)), .wait(forDuration: seconds)])
        }
        func frames(_ indices: [Int], _ perFrame: TimeInterval) -> SKAction {
            .animate(with: indices.map(frame), timePerFrame: perFrame)
        }

        switch state {
        case "walk":
            return .repeatForever(.animate(with: textures, timePerFrame: 0.12))
        case "idle":
            // Смотрит → моргает (прикрыла, закрыла) → смотрит иначе → снова моргает.
            return .repeatForever(.sequence([
                show(0, 2.5), show(1, 0.08), show(2, 0.14),
                show(3, 2.0), show(1, 0.08), show(2, 0.14),
            ]))
        case "working":
            // Печатает три цикла, потом задумывается с «?».
            return .repeatForever(.sequence([
                .repeat(frames([0, 1, 2], 0.25), count: 3), show(3, 1.2),
            ]))
        case "waiting":
            // Машет чаще остальных поз — зовёт одобрить.
            return .repeatForever(frames([0, 1, 2, 3], 0.3))
        case "done":
            // Ликует дважды и остаётся довольной: «готово» может висеть долго, без мельтешения.
            return .sequence([.repeat(frames([0, 1, 2], 0.3), count: 2), .setTexture(frame(3))])
        case "drag":
            // Удивилась, вскрикнула — и паникует, пока держат.
            return .sequence([frames([0, 1], 0.2), .repeatForever(frames([2, 3], 0.25))])
        default:
            // error: злость нарастает по кадрам.
            return .repeatForever(.animate(with: textures, timePerFrame: 0.4))
        }
    }
}
