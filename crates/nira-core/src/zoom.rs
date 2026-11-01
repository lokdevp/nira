//! Zoom / fit model.

use serde::{Deserialize, Serialize};

/// How pages are fitted into the viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FitMode {
    /// Fit the whole spread (width and height).
    #[default]
    Best,
    Width,
    Height,
    /// Fit to a fixed pixel size (`FitSize`).
    Size,
    /// No fitting: user zoom only.
    Manual,
}

impl FitMode {
    /// Every mode, in the order the UI lists them.
    pub const ALL: [FitMode; 5] = [
        FitMode::Best,
        FitMode::Width,
        FitMode::Height,
        FitMode::Size,
        FitMode::Manual,
    ];

    /// Stable name, as in `settings.toml` (`best`, `width`, …). Also the
    /// target of the UI's `win.fit` action.
    pub fn name(self) -> &'static str {
        match self {
            FitMode::Best => "best",
            FitMode::Width => "width",
            FitMode::Height => "height",
            FitMode::Size => "size",
            FitMode::Manual => "manual",
        }
    }

    pub fn from_name(name: &str) -> Option<FitMode> {
        FitMode::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Position in `ALL` (for dropdowns).
    pub fn index(self) -> u32 {
        FitMode::ALL.iter().position(|&m| m == self).unwrap_or(0) as u32
    }

    /// The mode at `index` in `ALL` (best fit if out of range).
    pub fn from_index(index: u32) -> FitMode {
        FitMode::ALL
            .get(index as usize)
            .copied()
            .unwrap_or_default()
    }
}

/// The fixed size used by `FitMode::Size`: which axis it limits, and to how
/// many pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FitSize {
    pub axis: FitSizeAxis,
    pub px: u32,
}

impl Default for FitSize {
    fn default() -> Self {
        FitSize {
            axis: FitSizeAxis::Height,
            px: 1800,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FitSizeAxis {
    Width,
    #[default]
    Height,
    /// Limit both axes.
    Both,
}

const IDENTITY_ZOOM_LOG: i32 = 0;
const MIN_USER_ZOOM_LOG: i32 = -20;
const MAX_USER_ZOOM_LOG: i32 = 12;
const USER_ZOOM_LOG_SCALE1: f64 = 4.0;

#[derive(Debug, Clone, Default)]
pub struct ZoomModel {
    pub fit_mode: FitMode,
    /// User zoom offset in logarithmic steps; `scale = 2^(log/4)`.
    pub user_zoom_log: i32,
    /// Whether fitting may upscale images beyond 100 %.
    pub scale_up: bool,
    pub fit_size: FitSize,
}

impl ZoomModel {
    pub fn zoom_in(&mut self) {
        self.user_zoom_log = (self.user_zoom_log + 1).min(MAX_USER_ZOOM_LOG);
    }

    pub fn zoom_out(&mut self) {
        self.user_zoom_log = (self.user_zoom_log - 1).max(MIN_USER_ZOOM_LOG);
    }

    pub fn reset_user_zoom(&mut self) {
        self.user_zoom_log = IDENTITY_ZOOM_LOG;
    }

    pub fn user_scale(&self) -> f64 {
        2f64.powf(self.user_zoom_log as f64 / USER_ZOOM_LOG_SCALE1)
    }

    /// Compute the on-screen size of each visible page.
    ///
    /// `image_sizes` are the full-resolution (rotated) sizes of the pages on
    /// the current spread, `viewport` is the available drawing area,
    /// `spacing` the gap between pages in double-page mode.
    pub fn zoomed_sizes(
        &self,
        image_sizes: &[(u32, u32)],
        viewport: (f64, f64),
        spacing: f64,
        double_page: bool,
    ) -> Vec<(u32, u32)> {
        if image_sizes.is_empty() {
            return Vec::new();
        }
        let (sw, sh) = viewport;
        let sw = if sw > 0.0 { sw } else { 1.0 };
        let sh = if sh > 0.0 { sh } else { 1.0 };

        // Union size of the spread (pages distributed along the width axis).
        let uw = if double_page {
            image_sizes.iter().map(|s| s.0 as f64).sum::<f64>() + spacing
        } else {
            image_sizes[0].0 as f64
        };
        let uh = image_sizes
            .iter()
            .map(|s| s.1 as f64)
            .fold(0.0_f64, f64::max);

        let fixed = self.fit_size.px as f64;
        let (limit_w, limit_h): (Option<f64>, Option<f64>) = match self.fit_mode {
            FitMode::Manual => (None, None),
            FitMode::Width => (Some(sw), None),
            FitMode::Height => (None, Some(sh)),
            FitMode::Best => (Some(sw), Some(sh)),
            FitMode::Size => match self.fit_size.axis {
                FitSizeAxis::Width => (Some(fixed), None),
                FitSizeAxis::Height => (None, Some(fixed)),
                FitSizeAxis::Both => (Some(fixed), Some(fixed)),
            },
        };

        let mut scale: f64 = 1.0;
        if let Some(lw) = limit_w {
            scale = scale.min(lw / uw);
        }
        if let Some(lh) = limit_h {
            scale = scale.min(lh / uh);
        }
        // Do not upscale small images unless the user zooms in.
        if !self.scale_up && self.fit_mode != FitMode::Manual {
            scale = scale.min(1.0);
        }
        scale *= self.user_scale();

        image_sizes
            .iter()
            .map(|(w, h)| {
                (
                    ((*w as f64) * scale).round().max(1.0) as u32,
                    ((*h as f64) * scale).round().max(1.0) as u32,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips_mode() {
        for mode in FitMode::ALL {
            assert_eq!(FitMode::from_index(mode.index()), mode);
        }
        assert_eq!(FitMode::from_index(99), FitMode::Best);
        for mode in FitMode::ALL {
            assert_eq!(FitMode::from_name(mode.name()), Some(mode));
        }
        // The names match the settings file.
        let toml = toml::to_string(&crate::settings::Settings::default()).unwrap();
        assert!(toml.contains("fit = \"best\""));
    }

    #[test]
    fn best_fit_scales_down_but_not_up() {
        let z = ZoomModel::default();
        assert_eq!(
            z.zoomed_sizes(&[(2000, 1000)], (1000.0, 1000.0), 0.0, false),
            vec![(1000, 500)]
        );
        assert_eq!(
            z.zoomed_sizes(&[(200, 100)], (1000.0, 1000.0), 0.0, false),
            vec![(200, 100)]
        );
    }

    #[test]
    fn fit_size_limits_the_chosen_axis() {
        let z = ZoomModel {
            fit_mode: FitMode::Size,
            fit_size: FitSize {
                axis: FitSizeAxis::Height,
                px: 500,
            },
            ..ZoomModel::default()
        };
        assert_eq!(
            z.zoomed_sizes(&[(4000, 1000)], (100.0, 100.0), 0.0, false),
            vec![(2000, 500)]
        );
    }
}
