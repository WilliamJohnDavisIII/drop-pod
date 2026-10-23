// Snapshot an SVG or HTML file to a PNG with WebKit, at an exact pixel size.
//   render <in.svg|in.html> <out.png> <width> [height]
// SVGs are inlined into a transparent page; HTML files load with read access to their folder.
import Cocoa
import WebKit

let args = CommandLine.arguments
guard args.count >= 4, let width = Int(args[3]) else {
    FileHandle.standardError.write("usage: render <in.svg|in.html> <out.png> <width> [height]\n".data(using: .utf8)!)
    exit(2)
}
let input = URL(fileURLWithPath: args[1]).standardizedFileURL
let output = URL(fileURLWithPath: args[2]).standardizedFileURL
let height = args.count > 4 ? Int(args[4])! : width
let job = Job(input: input, output: output, width: width, height: height)

struct Job {
    let input: URL, output: URL, width: Int, height: Int
}

final class Snapper: NSObject, WKNavigationDelegate {
    let web: WKWebView
    let window: NSWindow
    let job: Job

    init(_ job: Job) {
        self.job = job
        let (input, width, height) = (job.input, job.width, job.height)
        let frame = NSRect(x: 0, y: 0, width: width, height: height)
        web = WKWebView(frame: frame, configuration: WKWebViewConfiguration())
        web.setValue(false, forKey: "drawsBackground")
        web.underPageBackgroundColor = .clear
        window = NSWindow(contentRect: frame.offsetBy(dx: -20000, dy: -20000), styleMask: .borderless,
                          backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.contentView = web
        super.init()
        web.navigationDelegate = self
        window.orderFrontRegardless()

        if input.pathExtension.lowercased() == "svg" {
            let svg = try! String(contentsOf: input, encoding: .utf8)
            let html = """
            <!doctype html><html><head><style>
            html,body{margin:0;padding:0;background:transparent;overflow:hidden}
            svg{display:block;width:\(width)px;height:\(height)px}
            </style></head><body>\(svg)</body></html>
            """
            web.loadHTMLString(html, baseURL: nil)
        } else {
            web.loadFileURL(input, allowingReadAccessTo: input.deletingLastPathComponent())
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // Give images and filters a moment to decode before the snapshot.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { self.snap() }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        fail("load failed: \(error)")
    }

    func snap() {
        let (output, width, height) = (job.output, job.width, job.height)
        let config = WKSnapshotConfiguration()
        config.rect = CGRect(x: 0, y: 0, width: width, height: height)
        config.afterScreenUpdates = true
        web.takeSnapshot(with: config) { image, error in
            guard let image, let src = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
                fail("snapshot failed: \(String(describing: error))")
            }
            // The snapshot comes back at the screen's backing scale; resample to the exact size asked for.
            let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            ctx.interpolationQuality = .high
            ctx.draw(src, in: CGRect(x: 0, y: 0, width: width, height: height))
            let dest = CGImageDestinationCreateWithURL(output as CFURL, "public.png" as CFString, 1, nil)!
            CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
            guard CGImageDestinationFinalize(dest) else { fail("could not write \(output.path)") }
            exit(0)
        }
    }
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let snapper = Snapper(job)
DispatchQueue.main.asyncAfter(deadline: .now() + 30) { fail("timed out") }
app.run()
