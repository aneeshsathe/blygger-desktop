// The on-screen window ids (layer 0, taller than 100 pt) of one process:
//
//   swift scripts/verify/winid.swift <pid>
//
// burrow-verify.sh compiles this once and passes an id to
// `screencapture -l <id>`, so only the app's own windows are captured.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
  FileHandle.standardError.write("usage: winid <pid>\n".data(using: .utf8)!)
  exit(2)
}
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
  guard (w[kCGWindowOwnerPID as String] as? Int32) == pid,
    (w[kCGWindowLayer as String] as? Int) == 0,
    let b = w[kCGWindowBounds as String] as? [String: Any],
    let h = b["Height"] as? Double, h > 100,
    let id = w[kCGWindowNumber as String]
  else { continue }
  print(id)
}
