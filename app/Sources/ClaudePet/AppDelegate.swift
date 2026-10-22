import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var statusItem: NSStatusItem?
    private var pet: PetController?
    private let toggleItem = NSMenuItem(title: "Скрыть питомца", action: #selector(togglePet), keyEquivalent: "")
    private var statusItems: [NSMenuItem] = []
    private let bridge = BridgeClient()
    private let notifier = SessionNotifier()
    private var sessionItems: [NSMenuItem] = []
    private lazy var quickChatModel = QuickChatModel(bridge: bridge)
    private lazy var quickChat = QuickChatController(model: quickChatModel)
    private let bridgeItem = NSMenuItem(title: "Мост: запускается…", action: nil, keyEquivalent: "")
    private let hooksItem = NSMenuItem(title: "", action: #selector(toggleHooks), keyEquivalent: "")
    private var hooksStatus: HooksInstaller.Status?
    private let onboarding = OnboardingController()
    private let usageBadge = UsageBadge()
    private let usageItem = NSMenuItem(title: "", action: #selector(refreshUsage), keyEquivalent: "")
    private var lastSessions: [BridgeSession] = []
    private var petStatusResolver = PetStatusResolver()
    private var resolvedPetStatus: PetStatus?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let view = PetView()
        let scene = PetScene(sheet: SpriteSheet(), size: view.frame.size)
        // Капсула лимита — полоской под ногами: спрайт стоит вплотную к нижнему краю кадра.
        let petSize = PetView.displaySize
        let content = PetChromeView(frame: NSRect(x: 0, y: 0, width: petSize, height: petSize))
        view.frame = NSRect(x: 0, y: 0, width: petSize, height: petSize)
        content.addSubview(view)
        content.addSubview(usageBadge)
        let panel = PetPanel(content: content)
        usageBadge.onVisibilityChange = { [weak panel, weak content, weak usageBadge] visible in
            guard let panel, let content else { return }
            let inset = visible ? UsageBadge.height : 0
            let height = petSize + inset
            content.frame = NSRect(x: 0, y: 0, width: petSize, height: height)
            panel.setFrame(NSRect(origin: panel.frame.origin, size: NSSize(width: petSize, height: height)), display: true)
            usageBadge?.layoutInChrome()
        }
        usageBadge.onRefreshRequested = { [weak self] in self?.refreshUsageBadge() }
        let controller = PetController(panel: panel, scene: scene)
        view.controller = controller
        controller.onClick = { [weak self, weak controller] in
            guard let panel = controller?.panel else { return }
            self?.quickChat.toggle(near: panel)
        }
        view.menu = makePetContextMenu()
        view.presentScene(scene)
        controller.spawn()
        pet = controller

        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = NSImage(systemSymbolName: "pawprint.fill", accessibilityDescription: "Claude Pet")
        item.menu = makeMenu()
        item.menu?.delegate = self
        statusItem = item

        // Статус питомца считаем сами из списка сессий (PetStatusResolver), а не берём pet_status моста.
        // Один снимок — трём потребителям: меню (шаг 4), уведомления (шаг 4), Quick Chat (шаг 5).
        bridge.onSessions = { [weak self] sessions in
            self?.lastSessions = sessions
            self?.refreshPetStatus()
            self?.showSessions(sessions)
            self?.notifier.update(sessions)
            self?.quickChatModel.update(sessions: sessions)
            self?.refreshUsageBadge()
        }
        bridge.onUsage = { [weak self] _ in self?.refreshUsageBadge() }
        startUsagePolling()
        startPetStatusPolling()
        notifier.isChatOpen = { [weak self] id in self?.quickChatModel.isShowing(id) ?? false }
        notifier.onOpen = { [weak self] id in self?.openChat(sessionId: id) }
        notifier.requestAuthorization()
        bridge.onStateChange = { [weak self] state in
            self?.showBridgeState(state)
            self?.quickChatModel.bridgeStateChanged(state)
        }
        bridge.start()

        onboarding.model.onHooksChanged = { [weak self] status in self?.showHooksStatus(status) }
        onboarding.showOnFirstLaunch()
    }

    func applicationWillTerminate(_ notification: Notification) {
        pet?.savePosition()
        bridge.stop()
    }

    /// Статус питомца из текущих сессий. Применяется только при смене: ручной статус из меню «Статус (тест)»
    /// держится, пока вычисленный статус не изменится.
    private func refreshPetStatus() {
        let viewing = quickChatModel.selectedId.flatMap { quickChatModel.isShowing($0) ? $0 : nil }
        let status = petStatusResolver.resolve(lastSessions, now: Date(), viewing: viewing)
        guard status != resolvedPetStatus else { return }
        resolvedPetStatus = status
        apply(status)
    }

    /// «Свежее Готово» устаревает само, а открытый чат отмечает его просмотренным — без новых снимков моста.
    private func startPetStatusPolling() {
        Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                self?.refreshPetStatus()
            }
        }
    }

    /// Клик по уведомлению: чат питомца на нужной сессии (питомец, если был скрыт, возвращается на экран).
    private func openChat(sessionId: String) {
        guard let panel = pet?.panel else { return }
        if !panel.isVisible { togglePet() }
        quickChatModel.select(sessionId)
        quickChat.show(near: panel)
    }

    /// Лимит из `usage.json` — не только из моста (в `.app` иногда старый `pet-bridge`).
    private func refreshUsageBadge() {
        // Окно сброшено — effective пуст; тогда капсула показывает «—» по последним известным данным.
        guard let usage = UsageStore.effective(sessions: lastSessions) ?? UsageStore.load() else { return }
        usageBadge.update(usage)
    }

    private func startUsagePolling() {
        refreshUsageBadge()
        Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                self?.refreshUsageBadge()
            }
        }
    }

    /// Правый клик по питомцу, как у ChatGPT Pets: меню со «Скрыть».
    private func makePetContextMenu() -> NSMenu {
        let menu = NSMenu()
        let hide = NSMenuItem(title: "Скрыть питомца", action: #selector(togglePet), keyEquivalent: "")
        hide.target = self
        menu.addItem(hide)
        return menu
    }

    private func makeMenu() -> NSMenu {
        let menu = NSMenu()
        toggleItem.target = self
        menu.addItem(toggleItem)
        bridgeItem.isEnabled = false
        menu.addItem(bridgeItem)
        usageItem.target = self
        menu.addItem(usageItem)
        usageItem.title = UsageStore.describe(UsageStore.load())

        // Ручная проверка анимаций; следующий снимок моста статус перебивает.
        let statusMenu = NSMenu()
        for status in PetStatus.allCases {
            let entry = NSMenuItem(title: status.title, action: #selector(selectStatus(_:)), keyEquivalent: "")
            entry.target = self
            entry.representedObject = status.rawValue
            entry.state = status == .idle ? .on : .off
            statusMenu.addItem(entry)
            statusItems.append(entry)
        }
        let statusRoot = NSMenuItem(title: "Статус (тест)", action: nil, keyEquivalent: "")
        statusRoot.submenu = statusMenu
        menu.addItem(statusRoot)

        menu.addItem(.separator())
        hooksItem.target = self
        menu.addItem(hooksItem)
        showHooksStatus(HooksInstaller.status())
        let setup = NSMenuItem(title: "Настройка…", action: #selector(showOnboarding), keyEquivalent: "")
        setup.target = self
        menu.addItem(setup)
        menu.addItem(NSMenuItem(title: "Выйти", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        return menu
    }

    /// Строка про лимит в меню — по свежим данным в момент открытия.
    func menuWillOpen(_ menu: NSMenu) {
        usageItem.title = UsageStore.describe(UsageStore.load())
    }

    @objc private func refreshUsage() {
        refreshUsageBadge()
        usageItem.title = UsageStore.describe(UsageStore.load())
    }

    @objc private func showOnboarding() {
        onboarding.show()
    }

    @objc private func toggleHooks() {
        showHooksStatus(HooksInstaller.toggle(current: hooksStatus))
    }

    private func showHooksStatus(_ status: HooksInstaller.Status?) {
        hooksStatus = status
        switch status {
        case .installed: hooksItem.title = "Подключён ко всем проектам ✓ — отключить…"
        case .partial: hooksItem.title = "Подключён частично — подключить заново…"
        case .notInstalled: hooksItem.title = "Подключить ко всем проектам…"
        case nil: hooksItem.title = "Подключение: статус неизвестен"
        }
    }

    @objc private func togglePet() {
        guard let panel = pet?.panel else { return }
        if panel.isVisible {
            panel.orderOut(nil)
            toggleItem.title = "Показать питомца"
        } else {
            panel.orderFrontRegardless()
            toggleItem.title = "Скрыть питомца"
        }
    }

    @objc private func selectStatus(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let status = PetStatus(rawValue: raw) else { return }
        apply(status)
    }

    private func apply(_ status: PetStatus) {
        statusItems.forEach { $0.state = $0.representedObject as? String == status.rawValue ? .on : .off }
        pet?.setStatus(status)
    }

    /// Список сессий под строкой моста (аналог колокольчика ChatGPT): ждущие одобрения — первыми.
    private func showSessions(_ sessions: [BridgeSession]) {
        guard let menu = statusItem?.menu else { return }
        sessionItems.forEach(menu.removeItem)
        let sorted = sessions.filter { $0.status == .waiting } + sessions.filter { $0.status != .waiting }
        let titles = sorted.map { "\($0.displayName) · \($0.sourceTitle) · \($0.status.title)" }
        sessionItems = (titles.isEmpty ? ["Нет сессий"] : titles).map {
            let entry = NSMenuItem(title: $0, action: nil, keyEquivalent: "")
            entry.isEnabled = false // открытие сессии — позже
            return entry
        }
        let index = menu.index(of: bridgeItem) + 1
        for (offset, entry) in sessionItems.enumerated() { menu.insertItem(entry, at: index + offset) }
    }

    private func showBridgeState(_ state: BridgeClient.State) {
        switch state {
        case .notRunning:
            bridgeItem.title = "Мост: не запущен"
        case .running(let sessions):
            bridgeItem.title = "Сессий Claude: \(sessions)"
        }
    }
}
