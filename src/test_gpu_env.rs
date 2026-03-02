//! Test-only GPU environment helpers.
//!
//! These helpers avoid noisy backend probing on hosts that clearly cannot run
//! hardware GPU checks (for example Linux containers without `/dev/dri`).

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name),
        Ok(value) if matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    )
}

/// Return `true` when tests should skip GPU adapter probing on this host.
///
/// Set `COVERGEN_FORCE_GPU_TESTS=1` to force probes even when this function
/// would otherwise skip.
pub(crate) fn should_skip_gpu_adapter_probe() -> bool {
    if env_truthy("COVERGEN_FORCE_GPU_TESTS") {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, missing `/dev/dri` indicates no direct GPU device node.
        // Skip early to avoid recurring EGL/MESA backend warnings in headless
        // environments that cannot satisfy hardware GPU tests anyway.
        if !std::path::Path::new("/dev/dri").exists() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_gpu_tests_override_is_readable() {
        let _ = env_truthy("COVERGEN_FORCE_GPU_TESTS");
    }
}
