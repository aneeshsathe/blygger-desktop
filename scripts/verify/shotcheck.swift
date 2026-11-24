// Is a screenshot blank? A window captured while the Mac is locked (or one
// that drew nothing) comes back as one flat colour, and such a shot proves
// nothing about the screen it was meant to show.
//
//   shotcheck <png>                      prints its stats; exit 1 when blank
//   shotcheck --make <flat|drawn> <png>  writes a test image (the selftest's)
//
// Blank: fewer than BLANK_SHARE of the pixels differ (by more than TOLERANCE
// in any channel) from the image's most common colour. A real window, even an
// empty state, has its title, sidebar and text well above that.
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let BLANK_SHARE = 0.002
let TOLERANCE = 12

func fail(_ msg: String, _ code: Int32 = 2) -> Never {
  FileHandle.standardError.write((msg + "\n").data(using: .utf8)!)
  exit(code)
}

func rgba(width: Int, height: Int, fill: (inout [UInt8]) -> Void) -> CGImage {
  var px = [UInt8](repeating: 0, count: width * height * 4)
  fill(&px)
  let ctx = CGContext(
    data: &px, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
    space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  return ctx.makeImage()!
}

func write(_ image: CGImage, to path: String) {
  let url = URL(fileURLWithPath: path) as CFURL
  guard let dest = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil)
  else { fail("shotcheck: can't write \(path)") }
  CGImageDestinationAddImage(dest, image, nil)
  guard CGImageDestinationFinalize(dest) else { fail("shotcheck: can't write \(path)") }
}

let args = CommandLine.arguments
if args.count == 4, args[1] == "--make" {
  let (w, h) = (400, 300)
  let image = rgba(width: w, height: h) { px in
    for i in 0..<(w * h) {
      let (x, y) = (i % w, i / w)
      // "drawn": a flat page with a sidebar and lines of "text".
      let ink = args[2] == "drawn" && (x < 80 || (y % 20 < 3 && x > 100 && x < 360))
      let v: UInt8 = ink ? 40 : 245
      px[i * 4] = v
      px[i * 4 + 1] = v
      px[i * 4 + 2] = v
      px[i * 4 + 3] = 255
    }
  }
  write(image, to: args[3])
  exit(0)
}

guard args.count == 2 else { fail("usage: shotcheck <png> | shotcheck --make <flat|drawn> <png>") }
guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil),
  let image = CGImageSourceCreateImageAtIndex(src, 0, nil)
else { fail("shotcheck: can't read \(args[1])") }
let (w, h) = (image.width, image.height)
guard w > 0, h > 0 else { fail("shotcheck: \(args[1]) is empty", 1) }
let px = rgba(width: w, height: h) { px in
  let ctx = CGContext(
    data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
    space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
}
guard let data = px.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else {
  fail("shotcheck: can't read pixels of \(args[1])")
}
let n = w * h
var counts: [UInt32: Int] = [:]
for i in 0..<n {
  let o = i * 4
  let key =
    UInt32(bytes[o]) << 24 | UInt32(bytes[o + 1]) << 16 | UInt32(bytes[o + 2]) << 8
    | UInt32(bytes[o + 3])
  counts[key, default: 0] += 1
}
let mode = counts.max { $0.value < $1.value }!.key
let m = [Int(mode >> 24 & 0xff), Int(mode >> 16 & 0xff), Int(mode >> 8 & 0xff), Int(mode & 0xff)]
var differ = 0
for i in 0..<n {
  let o = i * 4
  for c in 0..<4 where abs(Int(bytes[o + c]) - m[c]) > TOLERANCE {
    differ += 1
    break
  }
}
let share = Double(differ) / Double(n)
let line = String(
  format: "%@: %dx%d, %d colours, %.4f%% of pixels off the main colour", args[1], w, h,
  counts.count, share * 100)
if share < BLANK_SHARE {
  fail("BLANK \(line)", 1)
}
print("ok \(line)")
