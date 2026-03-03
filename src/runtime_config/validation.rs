use std::error::Error;

use super::V2Config;

/// Validate cross-field runtime invariants after clap parsing and normalization.
pub(super) fn validate_v2_config(config: &V2Config) -> Result<(), Box<dyn Error>> {
    if config.width == 0 || config.height == 0 {
        return Err("width and height must be greater than zero".into());
    }
    if config.count == 0 {
        return Err("count must be at least 1".into());
    }
    if config.layers == 0 {
        return Err("layers must be at least 1".into());
    }
    if config.antialias == 0 || config.antialias > 4 {
        return Err("antialias must be in range 1..=4".into());
    }
    if config.animation.seconds == 0 {
        return Err("animation duration must be at least 1 second".into());
    }
    if config.animation.fps == 0 || config.animation.fps > 120 {
        return Err("fps must be in range 1..=120".into());
    }
    if config.gui.target_fps < 30 || config.gui.target_fps > 360 {
        return Err("gui-target-fps must be in range 30..=360".into());
    }
    if config.selection.explore_size < 16 {
        return Err("explore-size must be at least 16".into());
    }
    if config.animation.enabled && config.selection.enabled() {
        return Err("explore-candidates cannot be used with animation mode".into());
    }
    if config.manifest_in.is_some() && config.selection.enabled() {
        return Err("explore-candidates cannot be used with manifest replay mode".into());
    }
    Ok(())
}
