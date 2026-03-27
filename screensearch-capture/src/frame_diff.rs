//! Frame differencing to detect screen changes
//!
//! This module implements multiple algorithms for detecting changes between frames:
//! - Simple pixel-based comparison
//! - Histogram comparison for color distribution changes
//! - SSIM (Structural Similarity Index) for perceptual changes

use image::RgbaImage;

/// Method for calculating frame difference
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffMethod {
    /// Simple pixel comparison (fastest)
    Pixel,
    /// Histogram comparison (good for color changes)
    Histogram,
    /// SSIM - Structural Similarity Index (most accurate)
    Ssim,
}

/// Frame differencing engine
pub struct FrameDiffer {
    threshold: f32,
    last_frame: Option<RgbaImage>,
    method: DiffMethod,
}

impl FrameDiffer {
    /// Create a new frame differ with threshold and method
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            last_frame: None,
            method: DiffMethod::Histogram, // Default to histogram for good balance
        }
    }

    /// Create a new frame differ with specific method
    pub fn with_method(threshold: f32, method: DiffMethod) -> Self {
        Self {
            threshold,
            last_frame: None,
            method,
        }
    }

    /// Check if frame differs significantly from previous
    pub fn has_changed(&mut self, current: &RgbaImage) -> bool {
        let changed = match &self.last_frame {
            None => true,
            Some(last) => self.calculate_difference(last, current) > self.threshold,
        };

        if changed {
            self.last_frame = Some(current.clone());
        }

        changed
    }

    /// Calculate difference between two frames (0.0 - 1.0)
    fn calculate_difference(&self, frame1: &RgbaImage, frame2: &RgbaImage) -> f32 {
        if frame1.dimensions() != frame2.dimensions() {
            return 1.0;
        }

        match self.method {
            DiffMethod::Pixel => self.pixel_difference(frame1, frame2),
            DiffMethod::Histogram => self.histogram_difference(frame1, frame2),
            DiffMethod::Ssim => 1.0 - self.ssim(frame1, frame2),
        }
    }

    /// Simple pixel-based difference
    fn pixel_difference(&self, frame1: &RgbaImage, frame2: &RgbaImage) -> f32 {
        let total_pixels = (frame1.width() * frame1.height()) as f32;
        let mut diff_pixels = 0u32;

        for (p1, p2) in frame1.pixels().zip(frame2.pixels()) {
            if p1 != p2 {
                diff_pixels += 1;
            }
        }

        diff_pixels as f32 / total_pixels
    }

    /// Histogram-based difference (compares color distribution)
    fn histogram_difference(&self, frame1: &RgbaImage, frame2: &RgbaImage) -> f32 {
        const BINS: usize = 16; // 16 bins per channel
        let mut hist1 = [0u32; BINS * 3]; // RGB histograms
        let mut hist2 = [0u32; BINS * 3];

        // Build histograms
        for pixel in frame1.pixels() {
            let r = (pixel[0] as usize * BINS) / 256;
            let g = (pixel[1] as usize * BINS) / 256;
            let b = (pixel[2] as usize * BINS) / 256;
            hist1[r] += 1;
            hist1[BINS + g] += 1;
            hist1[BINS * 2 + b] += 1;
        }

        for pixel in frame2.pixels() {
            let r = (pixel[0] as usize * BINS) / 256;
            let g = (pixel[1] as usize * BINS) / 256;
            let b = (pixel[2] as usize * BINS) / 256;
            hist2[r] += 1;
            hist2[BINS + g] += 1;
            hist2[BINS * 2 + b] += 1;
        }

        // Calculate chi-squared distance
        let mut chi_squared = 0.0;
        for i in 0..hist1.len() {
            let h1 = hist1[i] as f32;
            let h2 = hist2[i] as f32;
            if h1 + h2 > 0.0 {
                chi_squared += ((h1 - h2) * (h1 - h2)) / (h1 + h2);
            }
        }

        // Normalize to 0-1 range
        let total_pixels = frame1.width() * frame1.height();
        (chi_squared / (total_pixels as f32)).min(1.0)
    }

    /// SSIM (Structural Similarity Index) calculation
    /// Returns value between 0.0 (completely different) and 1.0 (identical)
    fn ssim(&self, frame1: &RgbaImage, frame2: &RgbaImage) -> f32 {
        const WINDOW_SIZE: u32 = 8;
        const K1: f32 = 0.01;
        const K2: f32 = 0.03;
        const L: f32 = 255.0; // Dynamic range
        let c1 = (K1 * L) * (K1 * L);
        let c2 = (K2 * L) * (K2 * L);

        let width = frame1.width();
        let height = frame1.height();

        if width < WINDOW_SIZE || height < WINDOW_SIZE {
            return self.pixel_difference(frame1, frame2);
        }

        let mut ssim_sum = 0.0;
        let mut window_count = 0;

        // Sample windows across the image
        for y in (0..height - WINDOW_SIZE).step_by(WINDOW_SIZE as usize) {
            for x in (0..width - WINDOW_SIZE).step_by(WINDOW_SIZE as usize) {
                let ssim_window = self.ssim_window(frame1, frame2, x, y, WINDOW_SIZE, c1, c2);
                ssim_sum += ssim_window;
                window_count += 1;
            }
        }

        if window_count > 0 {
            ssim_sum / window_count as f32
        } else {
            0.0
        }
    }

    /// Calculate SSIM for a single window
    #[allow(clippy::too_many_arguments)]
    fn ssim_window(
        &self,
        frame1: &RgbaImage,
        frame2: &RgbaImage,
        x: u32,
        y: u32,
        size: u32,
        c1: f32,
        c2: f32,
    ) -> f32 {
        let mut sum1 = 0.0;
        let mut sum2 = 0.0;
        let mut sum1_sq = 0.0;
        let mut sum2_sq = 0.0;
        let mut sum12 = 0.0;
        let count = (size * size) as f32;

        for dy in 0..size {
            for dx in 0..size {
                let p1 = frame1.get_pixel(x + dx, y + dy);
                let p2 = frame2.get_pixel(x + dx, y + dy);

                // Convert to grayscale using luminance
                let gray1 = 0.299 * p1[0] as f32 + 0.587 * p1[1] as f32 + 0.114 * p1[2] as f32;
                let gray2 = 0.299 * p2[0] as f32 + 0.587 * p2[1] as f32 + 0.114 * p2[2] as f32;

                sum1 += gray1;
                sum2 += gray2;
                sum1_sq += gray1 * gray1;
                sum2_sq += gray2 * gray2;
                sum12 += gray1 * gray2;
            }
        }

        let mean1 = sum1 / count;
        let mean2 = sum2 / count;
        let var1 = (sum1_sq / count) - (mean1 * mean1);
        let var2 = (sum2_sq / count) - (mean2 * mean2);
        let covar = (sum12 / count) - (mean1 * mean2);

        let numerator = (2.0 * mean1 * mean2 + c1) * (2.0 * covar + c2);
        let denominator = (mean1 * mean1 + mean2 * mean2 + c1) * (var1 + var2 + c2);

        if denominator == 0.0 {
            1.0
        } else {
            numerator / denominator
        }
    }

    /// Reset the differ (clear last frame)
    pub fn reset(&mut self) {
        self.last_frame = None;
    }

    /// Get the current difference threshold
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Set a new difference threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    /// Get the current difference method
    pub fn method(&self) -> DiffMethod {
        self.method
    }

    /// Set a new difference method
    pub fn set_method(&mut self, method: DiffMethod) {
        self.method = method;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_differ_no_change() {
        let mut differ = FrameDiffer::new(0.05);
        let frame = RgbaImage::new(100, 100);

        assert!(differ.has_changed(&frame));
        assert!(!differ.has_changed(&frame));
    }

    #[test]
    fn test_frame_differ_with_change() {
        // Use pixel-based differ for this test since it's more deterministic
        let mut differ = FrameDiffer::with_method(0.005, DiffMethod::Pixel);
        let frame1 = RgbaImage::new(100, 100);
        let mut frame2 = RgbaImage::new(100, 100);

        // Modify 10% of pixels (100 out of 10000)
        for y in 0..10 {
            for x in 0..10 {
                frame2.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }

        assert!(differ.has_changed(&frame1));
        // 100 / 10000 = 0.01 (1%) which is > 0.005 threshold
        assert!(differ.has_changed(&frame2));
    }
}
