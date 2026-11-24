// The on-screen window ids (layer 0, taller than 100 pt) of one process.
// Off-screen windows are left out: a run with a web view also owns a blank
// 500x500 pt window that is never on screen (kCGWindowIsOnscreen unset), so
// users never see it, but listing it made those scenes capture a blank shot.
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
    (w[kCGWindowIsOnscreen as String] as? Bool) == true,
    let b = w[kCGWindowBounds as String] as? [String: Any],
    let h = b["Height"] as? Double, h > 100,
    let id = w[kCGWindowNumber as String]
  else { continue }
  print(id)
}
