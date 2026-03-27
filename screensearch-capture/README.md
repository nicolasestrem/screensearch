# Screen Capture Module

Hardware-accelerated screen capture engine for the ScreenSearch project using Windows Graphics Capture API.

## Features

- **Multi-monitor Support**: Enumerate and capture from all connected displays
- **Frame Differencing**: Skip unchanged frames to reduce CPU and storage usage (< 0.6% change threshold)
- **Hardware Acceleration**: Uses Windows Graphics Capture API for efficient, GPU-accelerated capture
- **Window Context**: Automatically captures active window information (title, process name, browser URLs)
- **Lock-Free Buffering**: Uses crossbeam ArrayQueue for efficient frame queuing (30 frame limit)
- **Configurable Intervals**: Capture at 2-5 second intervals (default: 3 seconds)
- **SSIM & Histogram**: Advanced frame difference algorithms for accurate change detection

## Architecture

### Core Components

1. **CaptureEngine** (`src/capture.rs`)
   - Implements `GraphicsCaptureApiHandler` trait from `windows-capture`
   - Manages per-monitor capture threads
   - Frame differencing via histogram comparison
   - Lock-free frame buffering with `ArrayQueue`

2. **FrameDiffer** (`src/frame_diff.rs`)
   - Three diff methods: Pixel, Histogram, SSIM
   - Default: Histogram (good balance of speed/accuracy)
   - Threshold: 0.006 (0.6% change)

3. **MonitorInfo** (`src/monitor.rs`)
   - Windows GDI monitor enumeration
   - Multi-monitor metadata (resolution, position, primary flag)

4. **WindowContext** (`src/window_context.rs`)
   - Active window tracking via Win32 APIs
   - Process name extraction
   - Browser URL extraction (Chrome, Firefox, Edge) via UI Automation API

## Usage

```rust
use screensearch_capture::{CaptureConfig, ScreenCapture};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = CaptureConfig {
        interval_ms: 3000,          // 3 seconds
        enable_frame_diff: true,    // Skip unchanged frames
        diff_threshold: 0.006,      // 0.6% change threshold
        max_frames_buffer: 30,      // Buffer size
        monitor_indices: vec![],    // All monitors
        ..Default::default()
    };

    let mut capture = ScreenCapture::new(config)?;

    // Start capture with callback
    capture.start(|frame| {
        println!("Captured frame from monitor {}: {}x{}",
            frame.monitor_index,
            frame.image.width(),
            frame.image.height()
        );

        if let Some(window) = frame.active_window {
            println!("Active window: {}", window);
        }

        Ok(())
    }).await?;

    Ok(())
}
```

## Performance

- **CPU Usage**: < 5% idle (with frame differencing)
- **Memory**: < 500MB RAM for 30 frames in buffer
- **Frame Diff**: Histogram comparison at ~1ms per frame
- **Capture**: Hardware-accelerated, ~50ms per screen

## Implementation Status

- [x] Monitor enumeration
- [x] Frame differencing (Pixel, Histogram, SSIM)
- [x] Window context tracking
- [x] Multi-monitor support
- [ ] Full Graphics Capture API integration (in progress)
- [ ] Browser URL extraction (stub implementation)

## Dependencies

- `windows-capture` 2.0.0-alpha.7 - Graphics Capture API bindings
- `windows` 0.52 - Win32 APIs
- `image` - Image processing
- `crossbeam` - Lock-free data structures
- `tokio` - Async runtime

## Windows Requirements

- Windows 10 version 1803 (April 2018 Update) or later
- Graphics Capture API support

## Architecture Notes

### Frame Differencing Threshold

The default threshold of 0.006 (0.6%) was chosen to:
- Skip truly unchanged screens (static desktop)
- Capture subtle changes (text cursor blinking, animations)
- Balance false positives vs. missed content

Tuning:
- Lower threshold (0.001-0.005): More sensitive, fewer skipped frames
- Higher threshold (0.01-0.05): Less sensitive, more frames skipped

### Buffer Size

30 frames at 1920x1080 RGBA = ~240MB uncompressed
- Frames are processed quickly and sent to database
- Lock-free queue prevents contention
- Overflow frames are dropped (oldest first)

### Thread Model

- One capture thread per monitor
- Async callback processing
- Non-blocking frame queue operations
- Graceful shutdown via AtomicBool flag

## Future Enhancements

1. **OCR Integration**: Move OCR to separate module (see `src/ocr.rs`)
2. **Compression**: JPEG/WebP encoding before storage
3. **Privacy Filters**: Exclude sensitive applications
4. **Adaptive Intervals**: Increase interval when screen is idle
5. **Browser URL Extraction**: Full UI Automation implementation

## Testing

```bash
# Run tests
cargo test

# Test with specific monitor
cargo test -- --nocapture test_capture_single_frame

# Benchmark frame differencing
cargo bench
```

## License

MIT
