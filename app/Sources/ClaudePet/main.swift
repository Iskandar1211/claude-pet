import AppKit

// NSApplication держит delegate слабой ссылкой — храним его здесь.
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
