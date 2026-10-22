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

    /// В WindowPet все состояния идут на 9 fps; для двухкадровых поз это мельтешение,
    /// поэтому ходьба быстрая, позы медленные, а моргание — короткое.
    private static func animation(_ state: String, _ textures: [SKTexture]) -> SKAction {
        switch state {
        case "walk":
            return .repeatForever(.animate(with: textures, timePerFrame: 0.12))
        case "idle":
            return .repeatForever(.sequence([
                .setTexture(textures[0]), .wait(forDuration: 2.5),
                .setTexture(textures[1]), .wait(forDuration: 0.15),
            ]))
        default:
            return .repeatForever(.animate(with: textures, timePerFrame: 0.45))
        }
    }
}
