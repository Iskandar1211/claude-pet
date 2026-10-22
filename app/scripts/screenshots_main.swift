// Рисует Quick Chat с ПРИМЕРНЫМИ данными в PNG для README. Не часть приложения:
// собирается скриптом render_screenshots.sh на копиях исходников (в копиях открыты private(set) модели).
import AppKit
import SwiftUI

func session(_ name: String, _ status: String, _ last: String, entrypoint: String,
             permission: String? = nil) -> BridgeSession {
    let perm = permission.map {
        #","pending_permission":{"request_id":"r1","tool_name":"Bash","input_preview":"\#($0)"}"#
    } ?? ""
    let json = #"{"session_id":"\#(name)","short_id":"\#(name)","entrypoint":"\#(entrypoint)","cwd":"/Users/dev/\#(name)","status":"\#(status)","last_message":"\#(last)","has_channel":\#(permission != nil)\#(perm)}"#
    return try! JSONDecoder().decode(BridgeSession.self, from: Data(json.utf8))
}

func message(_ json: String) -> BridgeClient.HistoryMessage {
    try! JSONDecoder().decode(BridgeClient.HistoryMessage.self, from: Data(json.utf8))
}

@MainActor func render(_ sessions: [BridgeSession], messages: [BridgeClient.HistoryMessage], to path: String) {
    let model = QuickChatModel(bridge: BridgeClient())
    model.update(sessions: sessions)
    model.selectedId = sessions[0].sessionId
    model.messages = messages
    let margin: CGFloat = 26 // место под тень
    let size = NSSize(width: QuickChatPanel.size.width + margin * 2, height: QuickChatPanel.size.height + margin * 2)
    let root = QuickChatView(model: model, onClose: {})
        .shadow(color: .black.opacity(0.45), radius: 14, y: 6)
        .padding(margin)
        .frame(width: size.width, height: size.height)
    let host = NSHostingView(rootView: root)
    host.frame = NSRect(origin: .zero, size: size)
    // Настоящая панель чата: она может стать key, и кнопки рисуются активными (как в приложении).
    let window = QuickChatPanel(content: host)
    window.setContentSize(size)
    window.contentView = host
    NSApp.setActivationPolicy(.accessory)
    NSApp.activate()
    window.makeKeyAndOrderFront(nil)
    RunLoop.main.run(until: Date().addingTimeInterval(0.8))
    let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds)!
    host.cacheDisplay(in: host.bounds, to: rep)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

let outDir = CommandLine.arguments[1]

MainActor.assumeIsolated {
    _ = NSApplication.shared

    render(
        [
            session("web-shop", "done", "assistant: Added the checkout form; all 12 tests pass.", entrypoint: "cli"),
            session("api-server", "working", "assistant: Refactoring the auth middleware…", entrypoint: "claude-vscode"),
        ],
        messages: [
            message(#"{"role":"user","kind":"text","text":"Add a checkout form to the cart page and cover it with tests."}"#),
            message(#"{"role":"assistant","kind":"text","text":"I'll add a **Checkout** component and wire it into the cart page:\n\n- form with address and card fields\n- validation before submit\n- tests for the happy path and for invalid input\n\nStarting with the component in `src/Checkout.tsx`."}"#),
            message(#"{"role":"assistant","kind":"tool","name":"Edit","summary":"src/Checkout.tsx"}"#),
            message(#"{"role":"assistant","kind":"tool","name":"Bash","summary":"npm test"}"#),
            message(#"{"role":"user","kind":"result","text":"PASS src/Checkout.test.tsx\nTests: 12 passed, 12 total"}"#),
            message(#"{"role":"assistant","kind":"text","text":"Done. The form lives in `src/Checkout.tsx` and all tests pass."}"#),
        ],
        to: outDir + "/quick-chat.png"
    )

    render(
        [session("web-shop", "waiting", "", entrypoint: "cli", permission: "npm install stripe && npm run build")],
        messages: [
            message(#"{"role":"user","kind":"text","text":"Add Stripe payments to the checkout."}"#),
            message(#"{"role":"assistant","kind":"text","text":"Plan: install the Stripe SDK, create a payment intent on the server, and confirm it from the checkout form."}"#),
            message(#"{"role":"assistant","kind":"tool","name":"Read","summary":"src/Checkout.tsx"}"#),
            message(#"{"role":"assistant","kind":"text","text":"I need to install the SDK first — waiting for your approval."}"#),
        ],
        to: outDir + "/approval.png"
    )
}
