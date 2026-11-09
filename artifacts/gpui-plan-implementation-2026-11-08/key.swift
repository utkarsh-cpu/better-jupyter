import AppKit
import CoreGraphics
import Foundation
let pid=pid_t(CommandLine.arguments[1])!
let key=CGKeyCode(CommandLine.arguments[2])!
let flags=CGEventFlags(rawValue:UInt64(CommandLine.arguments[3])!)
for down in [true,false] {let e=CGEvent(keyboardEventSource:nil,virtualKey:key,keyDown:down)!;e.flags=flags;e.postToPid(pid);usleep(100000)}
