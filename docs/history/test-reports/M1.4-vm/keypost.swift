// keypost <ropt|lopt|esc|shift-ropt> [holdMs]: posts synthetic key events at the HID tap.
import CoreGraphics
import Foundation
let what = CommandLine.arguments[1]
let hold = UInt32(CommandLine.arguments.count > 2 ? Int(CommandLine.arguments[2])! : 500)
let src = CGEventSource(stateID: .hidSystemState)
func post(_ key: CGKeyCode, _ down: Bool, flags: CGEventFlags, type: CGEventType) {
    let e = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: down)!
    e.type = type
    e.flags = flags
    e.post(tap: .cghidEventTap)
}
let alt = CGEventFlags.maskAlternate.rawValue
switch what {
case "ropt", "lopt":
    let (key, dev): (CGKeyCode, UInt64) = what == "ropt" ? (61, 0x40) : (58, 0x20)
    post(key, true, flags: CGEventFlags(rawValue: alt | dev | 0x100), type: .flagsChanged)
    usleep(hold * 1000)
    post(key, false, flags: CGEventFlags(rawValue: 0x100), type: .flagsChanged)
case "esc":
    post(53, true, flags: CGEventFlags(rawValue: 0x100), type: .keyDown)
    usleep(50_000)
    post(53, false, flags: CGEventFlags(rawValue: 0x100), type: .keyUp)
default: fatalError("unknown \(what)")
}
usleep(100_000)
print("posted \(what)")
