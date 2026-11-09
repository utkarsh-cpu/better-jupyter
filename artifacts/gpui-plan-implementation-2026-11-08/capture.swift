import CoreGraphics
import Foundation
let ws = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []
for w in ws { if (w[kCGWindowOwnerPID as String] as? Int) == Int(CommandLine.arguments[1]), let id = w[kCGWindowNumber as String] as? UInt32, (w[kCGWindowLayer as String] as? Int) == 0 { let p = Process(); p.executableURL = URL(fileURLWithPath:"/usr/sbin/screencapture"); p.arguments = ["-x", "-o", "-l", String(id), CommandLine.arguments[2]]; try p.run(); p.waitUntilExit(); break } }
